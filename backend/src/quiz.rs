use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use rand::Rng;
use serde::Deserialize;
use shakmaty::{Color, Position};
use sqlx::Row;

use crate::auth::user_from_jar;
use crate::error::{ApiError, ApiResult};
use crate::game::position_before_user_move;
use crate::lichess::ExplorerResponse;
use crate::models::{MastersStats, MoveResult, QuizPosition};
use crate::state::AppState;
use crate::tree::{build_tree, parse_position, quiz_anchors};

#[derive(Debug, Deserialize)]
pub struct StartBody {
    #[serde(rename = "openingId")]
    pub opening_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct MoveBody {
    pub uci: String,
}

#[axum::debug_handler(state = crate::state::AppState)]
pub async fn start(
    State(state): State<AppState>,
    jar: axum_extra::extract::CookieJar,
    Json(body): Json<StartBody>,
) -> ApiResult<(StatusCode, Json<QuizPosition>)> {
    let user = user_from_jar(&state, &jar).await?;
    let opening_id = body.opening_id;
    let row = sqlx::query("SELECT id FROM openings WHERE id = ? AND user_id = ?")
        .bind(opening_id)
        .bind(&user.id)
        .fetch_optional(&*state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("opening not found"))?;
    let _ = row;

    let quiz_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO quiz_sessions (quiz_id, user_id, opening_id) VALUES (?, ?, ?)")
        .bind(&quiz_id)
        .bind(&user.id)
        .bind(opening_id)
        .execute(&*state.pool)
        .await?;

    let position = generate_position(&state, &user, &quiz_id, opening_id).await?;
    Ok((StatusCode::CREATED, Json(position)))
}

pub async fn skip(
    State(state): State<AppState>,
    Path(quiz_id): Path<String>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<Json<QuizPosition>> {
    let user = user_from_jar(&state, &jar).await?;
    let row = sqlx::query("SELECT opening_id FROM quiz_sessions WHERE quiz_id = ? AND user_id = ?")
        .bind(&quiz_id)
        .bind(&user.id)
        .fetch_optional(&*state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("quiz not found"))?;
    let opening_id: i64 = row.get("opening_id");
    let position = generate_position(&state, &user, &quiz_id, opening_id).await?;
    Ok(Json(position))
}

/// Picks a random same-color anchor, queries the players DB, and stores the chosen
/// game position for the quiz.
async fn generate_position(
    state: &AppState,
    user: &crate::auth::User,
    quiz_id: &str,
    opening_id: i64,
) -> ApiResult<QuizPosition> {
    let row = sqlx::query("SELECT side, moves, range_min, range_max FROM openings WHERE id = ?")
        .bind(opening_id)
        .fetch_optional(&*state.pool)
        .await?
        .ok_or_else(|| ApiError::not_found("opening not found"))?;
    let side: String = row.get("side");
    let moves_json: String = row.get("moves");
    let moves: Vec<String> = serde_json::from_str(&moves_json).unwrap_or_default();
    let range_min: i64 = row.get("range_min");
    let range_max: i64 = row.get("range_max");

    let user_color = match side.as_str() {
        "white" => Color::White,
        _ => Color::Black,
    };
    let tree = build_tree(&moves);
    let anchors = quiz_anchors(&tree, user_color);
    if anchors.is_empty() {
        return Err(ApiError::conflict(
            "empty_opening",
            "this opening has no quizable positions yet",
        ));
    }
    let anchor = &anchors[rand::rng().random_range(0..anchors.len())];

    // The upcoming user move is the (user_moves + 1)-th; the range offsets it.
    let target_index = (anchor.user_moves as i64 + 1 + range_min).max(1);
    let offset = if range_max > range_min {
        rand::rng().random_range(0..=(range_max - range_min))
    } else {
        0
    };
    let target_index = (target_index + offset).max(1) as u32;

    // Load the user's search settings.
    let settings =
        sqlx::query("SELECT time_control, min_rating, max_rating FROM settings WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&*state.pool)
            .await?;
    let time_control: String = settings.get("time_control");
    let min_rating: i64 = settings.get("min_rating");
    let max_rating: i64 = settings.get("max_rating");

    let anchor_node = &tree.nodes[anchor.node];
    let games = state
        .lichess
        .players_games(
            &anchor_node.fen,
            &time_control,
            min_rating,
            max_rating,
            &user.token,
        )
        .await
        .map_err(|e| ApiError::internal(format!("players db query failed: {e}")))?;
    if games.is_empty() {
        return Err(ApiError::conflict(
            "no_games",
            "no games found for this position",
        ));
    }

    // Try a few random games until one contains the user's target move.
    let mut tried: Vec<String> = Vec::new();
    for game_id in random_order(games) {
        if tried.len() >= 5 {
            break;
        }
        tried.push(game_id.clone());
        let pgn = match state.lichess.game_pgn(&game_id, &user.token).await {
            Ok(pgn) => pgn,
            Err(e) => {
                tracing::warn!("game export {game_id} failed: {e}");
                continue;
            }
        };
        match position_before_user_move(&pgn, user_color, target_index) {
            Ok(Some(pos)) => {
                let quiz_ply = pos.ply;
                let side_str = if user_color == Color::White {
                    "white"
                } else {
                    "black"
                };
                let quiz_position = QuizPosition {
                    fen: pos.fen.clone(),
                    move_number: pos.move_number,
                    side: side_str.to_string(),
                    game_id: game_id.clone(),
                    game_url: format!("https://lichess.org/{game_id}#{}", pos.ply),
                };
                sqlx::query(
                    "INSERT INTO quiz_positions (quiz_id, fen_before, game_id, move_number, ply, user_is_white)
                     VALUES (?, ?, ?, ?, ?, ?)
                     ON CONFLICT(quiz_id) DO UPDATE SET
                        fen_before = excluded.fen_before,
                        game_id = excluded.game_id,
                        move_number = excluded.move_number,
                        ply = excluded.ply,
                        user_is_white = excluded.user_is_white",
                )
                .bind(quiz_id)
                .bind(&quiz_position.fen)
                .bind(&quiz_position.game_id)
                .bind(quiz_position.move_number as i64)
                .bind(quiz_ply as i64)
                .bind(i64::from(user_color == Color::White))
                .execute(&*state.pool)
                .await?;
                return Ok(quiz_position);
            }
            Ok(None) => continue,
            Err(e) => {
                tracing::warn!("failed to replay {game_id}: {e}");
                continue;
            }
        }
    }
    Err(ApiError::conflict(
        "no_games",
        "no usable game reached this position",
    ))
}

fn random_order(mut items: Vec<String>) -> Vec<String> {
    if !items.is_empty() {
        let n = items.len();
        for i in (1..n).rev() {
            items.swap(i, rand::rng().random_range(0..=i));
        }
    }
    items
}

pub async fn play_move(
    State(state): State<AppState>,
    Path(quiz_id): Path<String>,
    jar: axum_extra::extract::CookieJar,
    Json(body): Json<MoveBody>,
) -> ApiResult<Json<MoveResult>> {
    let user = user_from_jar(&state, &jar).await?;
    let row = sqlx::query(
        "SELECT p.fen_before, p.game_id, p.user_is_white
         FROM quiz_positions p JOIN quiz_sessions s ON s.quiz_id = p.quiz_id
         WHERE p.quiz_id = ? AND s.user_id = ?",
    )
    .bind(&quiz_id)
    .bind(&user.id)
    .fetch_optional(&*state.pool)
    .await?
    .ok_or_else(|| ApiError::not_found("quiz position not found"))?;
    let fen_before: String = row.get("fen_before");
    let _game_id: String = row.get("game_id");

    // Validate the played move and compute the resulting position.
    let before_position = parse_position(&fen_before)
        .map_err(|_| ApiError::internal("stored quiz position has an invalid FEN"))?;
    let uci = body
        .uci
        .parse::<shakmaty::uci::UciMove>()
        .map_err(|_| ApiError::bad_request("uci is not well-formed"))?;
    let m = uci
        .to_move(&before_position)
        .map_err(|_| ApiError::bad_request("illegal move in this position"))?;
    let san = crate::tree::san_of(&before_position, &m);
    let played_uci = crate::tree::uci_of(&m);
    let mut after_position = before_position.clone();
    after_position.play_unchecked(m);
    let fen_after = crate::tree::position_fen(&after_position);

    // Masters statistics for the position before the move.
    let masters_res: ExplorerResponse = state
        .lichess
        .masters(&fen_before, Some(&user.token))
        .await
        .map_err(|e| ApiError::internal(format!("masters explorer failed: {e}")))?;
    let masters: MastersStats = state.lichess.masters_stats(&masters_res, &san, &played_uci);

    // Eval delta from a single source (cloud-eval first, chess-api fallback).
    let eval = crate::eval::eval_delta(
        &state.lichess,
        &fen_before,
        &before_position,
        &fen_after,
        &after_position,
        &played_uci,
    )
    .await
    .map_err(|e| ApiError::internal(format!("eval lookup failed: {e}")))?;

    Ok(Json(MoveResult { masters, eval }))
}
