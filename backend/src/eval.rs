//! Engine evals, White POV. Lichess cloud-eval first, stockfish.online fallback —
//! never mixed: both numbers of a delta always come from the same source.

use shakmaty::{Chess, Move};

use crate::lichess::Lichess;
use crate::models::{EvalDelta, EvalValue};
use crate::tree::uci_of;

/// The best (first) PV of a cloud-eval response.
pub struct CloudBefore {
    lines: Vec<(String, EvalValue)>,
}

fn cloud_value(cp: Option<i64>, mate: Option<i32>) -> EvalValue {
    if let Some(mate) = mate {
        EvalValue::Mate { mate }
    } else {
        EvalValue::Pawns(cp.unwrap_or(0) as f64 / 100.0)
    }
}

/// Cloud-eval PVs for `fen`: each PV's first ply is a candidate move and its
/// cp/mate is the eval after that move (White POV). Castling UCI is normalized
/// from the explorer's king-takes-rook form to king-two-squares.
pub async fn cloud_lines(
    lichess: &Lichess,
    fen: &str,
    position: &Chess,
) -> Result<Option<CloudBefore>, anyhow::Error> {
    let Some(res) = lichess.cloud_eval(fen).await? else {
        return Ok(None);
    };
    let mut lines = Vec::new();
    for pv in &res.pvs {
        let first = pv.moves.split(' ').next().unwrap_or_default();
        let Ok(uci) = first.parse::<shakmaty::uci::UciMove>() else {
            continue;
        };
        let Some(m) = uci.to_move(position).ok() else {
            continue;
        };
        let normalized = if matches!(m, Move::Castle { .. }) {
            explorer_castle_uci(&m)
        } else {
            uci_of(&m)
        };
        lines.push((normalized, cloud_value(pv.cp, pv.mate)));
    }
    Ok(Some(CloudBefore { lines }))
}

fn explorer_castle_uci(m: &Move) -> String {
    crate::tree::explorer_uci_of(m)
}

/// Eval delta for the played move: cloud-eval for the before-position (whose PVs
/// usually already contain the after-eval), else cloud-eval of the after-position;
/// if cloud-eval misses for either, stockfish.online for both. Infallible: a failed
/// lookup degrades to an `unavailable` delta (the quiz move must never fail
/// because a third-party eval service is down).
pub async fn eval_delta(
    lichess: &Lichess,
    before_fen: &str,
    before_position: &Chess,
    after_fen: &str,
    after_position: &Chess,
    played_uci: &str,
) -> EvalDelta {
    match eval_delta_inner(
        lichess,
        before_fen,
        before_position,
        after_fen,
        after_position,
        played_uci,
    )
    .await
    {
        Ok(delta) => delta,
        Err(e) => {
            tracing::warn!("eval lookup failed, marking unavailable: {e:#}");
            EvalDelta {
                before: EvalValue::Unavailable,
                after: EvalValue::Unavailable,
                source: "unavailable",
            }
        }
    }
}

async fn eval_delta_inner(
    lichess: &Lichess,
    before_fen: &str,
    before_position: &Chess,
    after_fen: &str,
    after_position: &Chess,
    played_uci: &str,
) -> Result<EvalDelta, anyhow::Error> {
    if let Some(before) = cloud_lines(lichess, before_fen, before_position).await? {
        if let Some((_, after)) = before.lines.iter().find(|(uci, _)| uci == played_uci) {
            // The played move must have a before-value too: cloud-eval ranks PVs
            // best-first, so the before-value is the best line's value.
            if let Some((_, first)) = before.lines.first() {
                return Ok(EvalDelta {
                    before: *first,
                    after: *after,
                    source: "lichess-cloud",
                });
            }
        }
        let after = cloud_lines(lichess, after_fen, after_position).await?;
        let first = before.lines.first().map(|(_, v)| *v);
        let after_value = after
            .as_ref()
            .and_then(|a| a.lines.first())
            .map(|(_, v)| *v);
        if let (Some(before_val), Some(after_val)) = (first, after_value) {
            return Ok(EvalDelta {
                before: before_val,
                after: after_val,
                source: "lichess-cloud",
            });
        }
    }

    // Fallback: stockfish.online for both, keeping the numbers comparable.
    let (before, after) = tokio::try_join!(
        lichess.stockfish_eval(before_fen),
        lichess.stockfish_eval(after_fen)
    )?;
    let to_value = |(pawns, mate): (Option<f64>, Option<i32>)| match mate {
        Some(mate) => EvalValue::Mate { mate },
        None => EvalValue::Pawns(pawns.unwrap_or(0.0)),
    };
    Ok(EvalDelta {
        before: to_value(before),
        after: to_value(after),
        source: "stockfish.online",
    })
}
