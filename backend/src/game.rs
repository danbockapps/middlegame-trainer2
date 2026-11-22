//! Minimal PGN handling: extracts SAN tokens and replays them to find the
//! position before a given numbered move of the user's side.

use shakmaty::{CastlingMode, Chess, Color, Position, fen::Fen, san::San};

use crate::tree::position_fen;

/// A position before one of the user's moves in a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GamePosition {
    pub fen: String,
    /// Full move number the user is about to move (e.g. 8 = "move 8").
    pub move_number: u32,
    /// Ply index (1-based) of the user's upcoming move, for Lichess `#N` URLs.
    pub ply: u32,
}

/// Strips PGN headers, comments, variations, NAGs and move numbers, and returns
/// the SAN tokens in play order.
pub fn pgn_san_tokens(pgn: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut in_comment = false;
    for line in pgn.lines() {
        // Skip PGN header lines (tag pairs like [Event "Name"]).
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') && trimmed.contains('"') {
            continue;
        }
        for token in line.split_whitespace() {
            let mut token = token;
            // Strip inline comments first.
            let mut cleaned = String::new();
            for ch in token.chars() {
                match ch {
                    '{' => in_comment = true,
                    '}' => in_comment = false,
                    ';' => break,
                    _ if in_comment => {}
                    '(' => depth += 1,
                    ')' => depth = depth.saturating_sub(1),
                    _ if depth > 0 => {}
                    _ => cleaned.push(ch),
                }
            }
            token = cleaned.as_str();
            // Result tokens.
            if matches!(token, "1-0" | "0-1" | "1/2-1/2" | "*" | "½-½") {
                continue;
            }
            let token = token.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
            if token.is_empty() {
                continue;
            }
            tokens.push(token.to_string());
        }
    }
    tokens
}

/// Replays a PGN and returns the position before the user's `target`-th move
/// (1-based). When the game has fewer user moves than `target`, the last
/// user-move position is returned instead.
pub fn position_before_user_move(
    pgn: &str,
    user: Color,
    target: u32,
) -> Result<Option<GamePosition>, anyhow::Error> {
    let mut position: Chess = crate::tree::START_FEN
        .parse::<Fen>()
        .map_err(anyhow::Error::from)?
        .into_position(CastlingMode::Standard)
        .map_err(anyhow::Error::from)?;

    let mut user_count: u32 = 0;
    let mut found: Option<GamePosition> = None;

    for token in pgn_san_tokens(pgn) {
        if position.turn() == user {
            let upcoming_move_number = position.fullmoves().get();
            let upcoming_ply = (upcoming_move_number - 1) * 2
                + if position.turn() == Color::Black {
                    1
                } else {
                    0
                }
                + 1;
            if user_count + 1 == target {
                found = Some(GamePosition {
                    fen: position_fen(&position),
                    move_number: upcoming_move_number,
                    ply: upcoming_ply,
                });
            }
            user_count += 1;
        }
        let san: San = token
            .parse()
            .map_err(|e| anyhow::anyhow!("bad SAN {token:?}: {e}"))?;
        let m = san
            .to_move(&position)
            .map_err(|_| anyhow::anyhow!("illegal SAN {token:?}"))?;
        position = position.play(m).map_err(anyhow::Error::from)?;
    }

    // If the game ended before the target user move, fall back to the last one.
    if found.is_none() && user_count > 0 {
        return position_before_user_move(pgn, user, user_count);
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PGN: &str = r#"
[Event "Casual game"]

1. e4 e5 2. Nf3 {main line} Nc6 3. Bb5 (3. d4 d5) a6 4. Ba4 Nf6 5. O-O 1-0
"#;

    #[test]
    fn tokenizes_headers_comments_and_variations_away() {
        let tokens = pgn_san_tokens(PGN);
        let expected = ["e4", "e5", "Nf3", "Nc6", "Bb5", "a6", "Ba4", "Nf6", "O-O"];
        assert_eq!(tokens, expected);
    }

    #[test]
    fn finds_position_before_user_move() {
        let found = position_before_user_move(PGN, Color::White, 3)
            .unwrap()
            .unwrap();
        assert_eq!(found.move_number, 3);
        // Position before white's 3rd move: after 1.e4 e5 2.Nf3 Nc6.
        let pos = found
            .fen
            .parse::<Fen>()
            .unwrap()
            .into_position::<Chess>(CastlingMode::Standard)
            .unwrap();
        assert_eq!(pos.turn(), Color::White);
        // The move played at the anchor position is Bb5.
        let tokens = pgn_san_tokens(PGN);
        assert_eq!(tokens[4], "Bb5");
    }

    #[test]
    fn clamps_target_to_last_user_move() {
        let found = position_before_user_move(PGN, Color::White, 99)
            .unwrap()
            .unwrap();
        assert_eq!(found.move_number, 5);
        assert_eq!(found.ply, 9);
    }

    #[test]
    fn empty_pgn_returns_none() {
        assert!(
            position_before_user_move("", Color::White, 1)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn ply_counts_halfmoves() {
        let found = position_before_user_move(PGN, Color::Black, 2)
            .unwrap()
            .unwrap();
        // Before black's 2nd move: after 1.e4 e5 2.Nf3 → ply of black's 2nd move = 4.
        assert_eq!(found.ply, 4);
    }
}
