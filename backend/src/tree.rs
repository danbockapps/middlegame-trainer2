//! Pure domain logic for opening move trees. No I/O — everything here is
//! unit-testable without network.

use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Move, Position, fen::Fen, san::San, uci::UciMove,
};

#[allow(dead_code)]
pub const START_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

/// A node in the opening move tree. The root has no move.
#[derive(Debug, Clone)]
pub struct TreeNode {
    /// UCI of the move leading here (king-two-squares castling form), `None` for the root.
    pub uci: Option<String>,
    /// FEN of the position *at* this node (after the move leading here).
    pub fen: String,
    /// The position itself, for legality checks of further moves.
    pub position: Chess,
    #[allow(dead_code)]
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Number of plies from the root (root = 0).
    pub depth: usize,
    /// Number of the move the side to move is about to make (root = 1).
    pub fullmove: u32,
}

#[derive(Debug, Clone)]
pub struct Tree {
    pub nodes: Vec<TreeNode>,
}

impl Tree {
    pub fn root(&self) -> usize {
        0
    }

    /// True if the side to move at `idx` is `color`.
    #[allow(dead_code)]
    pub fn is_turn(&self, idx: usize, color: Color) -> bool {
        self.nodes[idx].position.turn() == color
    }
}

/// Parses a FEN into a playable position.
pub fn parse_position(fen: &str) -> Result<Chess, anyhow::Error> {
    let fen: Fen = fen.parse()?;
    Ok(fen.into_position(CastlingMode::Standard)?)
}

/// Serializes a position to a FEN string (en-passant square only when legal, as Lichess expects).
pub fn position_fen(position: &Chess) -> String {
    Fen::from_position(position, EnPassantMode::Legal).to_string()
}

/// Standard UCI string for a legal move (castling as king-two-squares, e.g. `e1g1`).
pub fn uci_of(m: &Move) -> String {
    uci_move_of(m).to_string()
}

fn uci_move_of(m: &Move) -> UciMove {
    UciMove::from_move(*m, CastlingMode::Standard)
}

/// UCI string in the "king takes rook" form used by the Lichess explorer for
/// castling (e.g. `e1h1`). Same as [`uci_of`] for non-castling moves.
pub fn explorer_uci_of(m: &Move) -> String {
    match m {
        Move::Castle { rook, .. } => {
            // `rook` is the rook's destination square, which equals the king's
            // to-square in the king-takes-rook encoding.
            format!("{}{}", m.from().expect("castle has from square"), rook)
        }
        _ => uci_of(m),
    }
}

/// SAN string for a legal move in `position` (castling as `O-O`/`O-O-O`).
pub fn san_of(position: &Chess, m: &Move) -> String {
    San::from_move(position, *m).to_string()
}

fn child_with_uci(tree: &Tree, idx: usize, uci: &str) -> Option<usize> {
    tree.nodes[idx]
        .children
        .iter()
        .copied()
        .find(|&c| tree.nodes[c].uci.as_deref() == Some(uci))
}

/// Builds a tree from a DFS pre-order list of UCI moves (the contract's "Tree
/// serialization"): each move is attached to the deepest earlier position where it
/// is a legal, not-yet-present continuation; a move that is not legal at the
/// current position starts a new branch from the earliest legal ancestor.
/// Illegal moves that fit nowhere are dropped.
pub fn build_tree(moves: &[String]) -> Tree {
    let root_position = Chess::default();
    let mut tree = Tree {
        nodes: vec![TreeNode {
            uci: None,
            fen: position_fen(&root_position),
            position: root_position,
            parent: None,
            children: Vec::new(),
            depth: 0,
            fullmove: 1,
        }],
    };

    // Cursor path from root (first element) to the deepest node of the previous move.
    let mut cursor: Vec<usize> = vec![0];

    for uci in moves {
        // End-of-branch marker: pop the cursor back up one level (clamped at the root).
        // Emitted by the client's serializer after each child subtree; absent in legacy
        // lists, which keep the legacy lossy reconstruction.
        if uci == "-" {
            if cursor.len() > 1 {
                cursor.pop();
            }
            continue;
        }
        let parsed = match uci.parse::<UciMove>() {
            Ok(u) => u,
            Err(_) => continue,
        };
        let mut attached: Option<(usize, Vec<usize>)> = None;
        for (depth, &node) in cursor.iter().enumerate().rev() {
            if child_with_uci(&tree, node, uci).is_some() {
                // Already present here; the contract says attach where not present,
                // so keep looking further up.
                continue;
            }
            let Some(m) = parsed.to_move(&tree.nodes[node].position).ok() else {
                continue;
            };
            let mut position = tree.nodes[node].position.clone();
            position.play_unchecked(m);
            let fullmove = if tree.nodes[node].position.turn() == Color::Black {
                tree.nodes[node].fullmove + 1
            } else {
                tree.nodes[node].fullmove
            };
            let new_idx = tree.nodes.len();
            tree.nodes.push(TreeNode {
                uci: Some(uci.clone()),
                fen: position_fen(&position),
                position,
                parent: Some(node),
                children: Vec::new(),
                depth: tree.nodes[node].depth + 1,
                fullmove,
            });
            tree.nodes[node].children.push(new_idx);
            let mut path = cursor[..=depth].to_vec();
            path.push(new_idx);
            attached = Some((depth, path));
            break;
        }
        if let Some((depth, path)) = attached {
            // The new node becomes the cursor's deepest node.
            cursor.truncate(depth + 1);
            cursor.push(tree.nodes.len() - 1);
            debug_assert_eq!(cursor, path);
        }
        // Unattachable (illegal everywhere): leave the cursor unchanged and drop the move.
    }

    tree
}

/// A position where the user (playing `user`) is to move, that a quiz can be built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuizAnchor {
    /// Index of the node with the user's side to move (the FEN before the user's move).
    pub node: usize,
    /// Number of user moves already made on the path to `node`. The upcoming move
    /// is the user's `user_moves + 1`-th.
    pub user_moves: u32,
}

/// Finds all same-color quiz anchors: for every root-to-leaf path, the deepest node
/// whose side to move is the user's. If a path's deepest leaf is the opponent's turn,
/// this naturally selects the user's move preceding it.
pub fn quiz_anchors(tree: &Tree, user: Color) -> Vec<QuizAnchor> {
    if tree.nodes.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    walk(tree, tree.root(), user, 0, None, &mut |node, user_moves| {
        out.push(QuizAnchor { node, user_moves });
    });
    out
}

fn walk(
    tree: &Tree,
    idx: usize,
    user: Color,
    user_moves: u32,
    anchor: Option<usize>,
    emit: &mut impl FnMut(usize, u32),
) {
    let node = &tree.nodes[idx];
    let is_user_turn = node.position.turn() == user;
    let (anchor, user_moves) = if is_user_turn {
        (Some(idx), user_moves)
    } else {
        (anchor, user_moves)
    };
    if node.children.is_empty() {
        if let Some(a) = anchor {
            emit(a, user_moves);
        }
        return;
    }
    for &child in &node.children {
        let next_moves = if is_user_turn {
            user_moves + 1
        } else {
            user_moves
        };
        walk(tree, child, user, next_moves, anchor, emit);
    }
}

/// FEN at which the opening should be named: the shallowest node where the tree
/// splits into multiple continuations, or the deepest node of the (single) line
/// when there is no split.
pub fn naming_fen(tree: &Tree) -> String {
    let mut split: Option<(usize, &TreeNode)> = None;
    for node in &tree.nodes {
        if node.children.len() >= 2 && split.is_none_or(|(d, _)| node.depth < d) {
            split = Some((node.depth, node));
        }
    }
    if let Some((_, node)) = split {
        return node.fen.clone();
    }
    tree.nodes
        .iter()
        .max_by_key(|n| n.depth)
        .map(|n| n.fen.clone())
        .unwrap_or_else(|| position_fen(&Chess::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moves(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn linear_tree_builds_single_chain() {
        let tree = build_tree(&moves(&["e2e4", "e7e5", "g1f3"]));
        assert_eq!(tree.nodes.len(), 4);
        assert_eq!(tree.nodes[3].uci.as_deref(), Some("g1f3"));
        assert_eq!(tree.nodes[3].depth, 3);
        assert_eq!(tree.nodes[3].fen.split(' ').nth(1), Some("b"));
    }

    #[test]
    fn branch_off_earlier_position() {
        // DFS pre-order: the second black reply c7c5 is not legal at the deepest
        // earlier position (white to move there), so it branches from 1.e4.
        let tree = build_tree(&moves(&["e2e4", "e7e5", "c7c5"]));
        let e4 = tree.nodes[0].children[0];
        assert_eq!(tree.nodes[e4].children.len(), 2);
        let e5 = tree.nodes[e4].children[0];
        let c5 = tree.nodes[e4].children[1];
        assert_eq!(tree.nodes[e5].uci.as_deref(), Some("e7e5"));
        assert_eq!(tree.nodes[c5].uci.as_deref(), Some("c7c5"));
        assert_eq!(tree.nodes[c5].depth, 2);
        assert_eq!(tree.nodes[c5].fen.split(' ').nth(1), Some("w"));
    }

    #[test]
    fn duplicate_move_is_dropped() {
        let tree = build_tree(&moves(&["e2e4", "e2e4"]));
        assert_eq!(tree.nodes.len(), 2);
    }

    #[test]
    fn branch_marker_pops_cursor() {
        // `-` after a subtree pops the cursor, so the following move attaches to the
        // e4 node again instead of to the deepest node of the finished subtree.
        let tree = build_tree(&moves(&["e2e4", "e7e5", "g1f3", "-", "-", "c7c5"]));
        let e4 = tree.nodes[0].children[0];
        assert_eq!(tree.nodes[e4].children.len(), 2);
        assert_eq!(tree.nodes[e4].children[0], 2);
        assert_eq!(tree.nodes[e4].children[1], 4);
    }

    #[test]
    fn sibling_branches_with_same_first_move_stay_separate() {
        // The flat-list ambiguity case: both branches continue with g1f3, which is
        // legal in both positions. Without the `-` markers the second g1f3 would
        // attach to the wrong (deeper) position.
        let tree = build_tree(&moves(&["e2e4", "e7e5", "-", "c7c5", "g1f3"]));
        let e4 = tree.nodes[0].children[0];
        assert_eq!(tree.nodes[e4].children.len(), 2);
        let e5 = tree.nodes[e4].children[0];
        let c5 = tree.nodes[e4].children[1];
        // e5's branch ended without continuing; c5's branch has the g1f3 child.
        assert!(tree.nodes[e5].children.is_empty());
        assert_eq!(tree.nodes[c5].children.len(), 1);
        assert_eq!(
            tree.nodes[tree.nodes[c5].children[0]].uci.as_deref(),
            Some("g1f3")
        );
    }

    #[test]
    fn castling_uses_king_two_squares_uci() {
        // A fool's-mate-ish setup can't castle; instead go through a real sequence.
        let tree = build_tree(&moves(&[
            "e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6", "e1g1",
        ]));
        let last = tree.nodes.len() - 1;
        assert_eq!(tree.nodes[last].uci.as_deref(), Some("e1g1"));
        // The king ends on g1 and the rook on f1.
        let pos = &tree.nodes[last].position;
        let king = pos
            .board()
            .king_of(Color::White)
            .expect("white king present");
        assert_eq!(king.to_string(), "g1");
    }

    #[test]
    fn anchors_prefer_user_turn_leaves() {
        // White repertoire: e4, then black's e5, then white's Nf3 (leaf is black's turn).
        let tree = build_tree(&moves(&["e2e4", "e7e5", "g1f3", "b8c6"]));
        // Leaf is after black's b8c6 → white to move. Anchors should be the nodes
        // where white is to move: the root and the position after e7e5.
        let anchors = quiz_anchors(&tree, Color::White);
        let nodes: Vec<usize> = anchors.iter().map(|a| a.node).collect();
        // The only path is root->e4->e5->Nf3->b8c6; its leaf is white-to-move,
        // so the anchor is the b8c6 node itself (depth 4).
        assert_eq!(anchors.len(), 1);
        assert_eq!(nodes[0], 4);
        // Every anchor must be a white-to-move position.
        for a in &anchors {
            assert!(tree.is_turn(a.node, Color::White));
        }
        // The root is never an anchor here because each path continues with white moves.
        assert!(!nodes.contains(&0));
    }

    #[test]
    fn anchor_counts_user_moves() {
        // Two black replies to e4 make two paths; both anchors are after 1 white move.
        let tree = build_tree(&moves(&["e2e4", "e7e5", "e2e4", "c7c5"]));
        // after the second branch, tree has root, e4 (shared), e5 and c5.
        let anchors = quiz_anchors(&tree, Color::White);
        assert_eq!(anchors.len(), 2);
        for a in &anchors {
            assert_eq!(a.user_moves, 1);
        }
        // Black side: both paths share the black-to-move node after 1.e4.
        let anchors = quiz_anchors(&tree, Color::Black);
        assert_eq!(anchors.len(), 2);
        for a in &anchors {
            assert!(tree.is_turn(a.node, Color::Black));
            // Upcoming black move is their 2nd (after white's e4).
            assert_eq!(a.user_moves, 1);
        }
    }

    #[test]
    fn naming_uses_first_split() {
        // Tree: e4 / (e5) and (c5). First split is at the e4 node (position after e4).
        let tree = build_tree(&moves(&["e2e4", "e7e5", "c7c5"]));
        let fen = naming_fen(&tree);
        let pos = parse_position(&fen).expect("valid fen");
        assert_eq!(pos.turn(), Color::Black);
        // Position after 1.e4 → FEN board part matches.
        assert!(fen.starts_with("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR"));
    }

    #[test]
    fn naming_falls_back_to_line_end() {
        let tree = build_tree(&moves(&["e2e4", "e7e5", "g1f3"]));
        let fen = naming_fen(&tree);
        assert!(fen.starts_with("rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R"));
        assert!(fen.ends_with("b KQkq - 1 2"));
    }

    #[test]
    fn empty_tree_names_start_position() {
        let tree = build_tree(&moves(&[]));
        assert_eq!(naming_fen(&tree), START_FEN);
    }

    #[test]
    fn explorer_uci_matches_castling_encoding() {
        let mut after = Chess::default();
        for uci in ["e2e4", "e7e5", "g1f3", "b8c6", "f1c4", "g8f6"] {
            let m = uci
                .parse::<UciMove>()
                .unwrap()
                .to_move(&after)
                .expect("legal");
            after.play_unchecked(m);
        }
        let m = "e1g1"
            .parse::<UciMove>()
            .unwrap()
            .to_move(&after)
            .expect("legal castle");
        assert_eq!(explorer_uci_of(&m), "e1h1");
        assert_eq!(uci_of(&m), "e1g1");
        assert_eq!(san_of(&after, &m), "O-O");
    }
}
