//! All Lichess (and chess-api.com) access happens here, server-side, using the
//! logged-in user's OAuth token. The opening explorer 401s tokenless requests
//! from many IPs, so explorer calls always send the Bearer token when available.
//! Explorer calls retry once on HTTP 429.

use anyhow::{Result, anyhow};
use chrono::{Datelike, Utc};
use rand::Rng;
use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;
use std::time::{Duration, Instant};

use crate::models::{MastersMove, MastersStats};

const EXPLORER_BASE: &str = "https://explorer.lichess.ovh";
const LICHESS_BASE: &str = "https://lichess.org";
const CHESS_API: &str = "https://chess-api.com/v1";

/// Sends an outbound HTTP request, logging the request and the response (status +
/// duration) so external API latency/failures are visible in the server log.
async fn send_logged(http: &Client, req: RequestBuilder) -> Result<Response> {
    let req = req.build()?;
    let method = req.method().to_string();
    let url = req.url().to_string();
    tracing::info!("external api request: {method} {url}");
    let start = Instant::now();
    match http.execute(req).await {
        Ok(res) => {
            tracing::info!(
                "external api response: {method} {url} -> {} in {:?}",
                res.status(),
                start.elapsed()
            );
            Ok(res)
        }
        Err(e) => {
            tracing::warn!(
                "external api error: {method} {url} after {:?}: {e}",
                start.elapsed()
            );
            Err(e.into())
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExplorerMove {
    pub uci: String,
    pub san: String,
    #[serde(default)]
    pub white: i64,
    #[serde(default)]
    pub draws: i64,
    #[serde(default)]
    pub black: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpeningInfo {
    #[allow(dead_code)]
    pub eco: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GameRef {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExplorerResponse {
    #[serde(default)]
    pub white: i64,
    #[serde(default)]
    pub draws: i64,
    #[serde(default)]
    pub black: i64,
    #[serde(default)]
    pub moves: Vec<ExplorerMove>,
    #[serde(default, rename = "topGames")]
    pub top_games: Vec<GameRef>,
    #[serde(default, rename = "recentGames")]
    pub recent_games: Vec<GameRef>,
    #[serde(default)]
    pub opening: Option<OpeningInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LichessMe {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub perfs: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudPv {
    pub moves: String,
    #[serde(default)]
    pub cp: Option<i64>,
    #[serde(default)]
    pub mate: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloudEvalResponse {
    #[serde(default)]
    pub pvs: Vec<CloudPv>,
}

pub struct Lichess {
    http: Client,
}

impl Lichess {
    pub fn new() -> Self {
        let http = Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("middlegame-trainer/2")
            .build()
            .expect("reqwest client builds");
        Self { http }
    }

    /// GET JSON with one retry on 429 (honoring Retry-After when present).
    async fn get_json(&self, url: &str, token: Option<&str>) -> Result<serde_json::Value> {
        for attempt in 0..2 {
            let mut req = self.http.get(url);
            if let Some(token) = token {
                req = req.bearer_auth(token);
            }
            let res = send_logged(&self.http, req).await?;
            if res.status() == 429 && attempt == 0 {
                let wait = res
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(2);
                tokio::time::sleep(Duration::from_secs(wait.min(10))).await;
                continue;
            }
            let status = res.status();
            let body = res.bytes().await?;
            if !status.is_success() {
                return Err(anyhow!(
                    "GET {url} failed: {status}: {}",
                    String::from_utf8_lossy(&body)
                ));
            }
            return Ok(serde_json::from_slice(&body)?);
        }
        unreachable!("retry loop returns or errors")
    }

    async fn get_explorer(
        &self,
        path: &str,
        query: &str,
        token: Option<&str>,
    ) -> Result<ExplorerResponse> {
        let url = format!("{EXPLORER_BASE}/{path}?{query}");
        let value = self.get_json(&url, token).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Masters database response for a position.
    pub async fn masters(&self, fen: &str, token: Option<&str>) -> Result<ExplorerResponse> {
        let query = format!("fen={}", urlencode(fen));
        self.get_explorer("masters", &query, token).await
    }

    /// Players (amateur) database response for one month window.
    async fn players_month(
        &self,
        fen: &str,
        speeds: &str,
        ratings: &str,
        month: &str,
        token: &str,
    ) -> Result<ExplorerResponse> {
        let query = format!(
            "fen={}&speeds={}&ratings={}&since={}&until={}&moves=24",
            urlencode(fen),
            urlencode(speeds),
            urlencode(ratings),
            month,
            month
        );
        self.get_explorer("lichess", &query, Some(token)).await
    }

    /// Collects game ids from the players database, walking month windows in a
    /// random order (the explorer only returns a handful of games per month).
    /// Stops after finding at least one populated month's worth of games.
    pub async fn players_games(
        &self,
        fen: &str,
        speeds: &str,
        min_rating: i64,
        max_rating: i64,
        token: &str,
    ) -> Result<Vec<String>> {
        let ratings = rating_buckets(min_rating, max_rating);
        let mut months = candidate_months();
        if months.is_empty() {
            return Ok(Vec::new());
        }
        let start = rand::rng().random_range(0..months.len());
        months.rotate_left(start);

        let mut games: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (call, month) in months.iter().take(6).enumerate() {
            let res = self
                .players_month(fen, speeds, &ratings, month, token)
                .await;
            let res = match res {
                Ok(res) => res,
                Err(e) => {
                    // A single failed month shouldn't kill the search.
                    tracing::warn!("players db month {month} failed: {e}");
                    continue;
                }
            };
            for game in res.recent_games.into_iter().chain(res.top_games) {
                if seen.insert(game.id.clone()) {
                    games.push(game.id);
                }
            }
            // One populated month (~4 games) is enough to pick from.
            if !games.is_empty() && call >= 1 {
                break;
            }
        }
        Ok(games)
    }

    /// Exports a game's PGN.
    pub async fn game_pgn(&self, game_id: &str, token: &str) -> Result<String> {
        let url = format!("{LICHESS_BASE}/game/export/{game_id}");
        let mut req = self.http.get(&url).bearer_auth(token);
        req = req.header("Accept", "application/x-chessgo");
        for attempt in 0..2 {
            let res = send_logged(&self.http, req.try_clone().expect("request cloneable")).await?;
            if res.status() == 429 && attempt == 0 {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }
            let status = res.status();
            if !status.is_success() {
                return Err(anyhow!("game export {game_id} failed: {status}"));
            }
            return Ok(res.text().await?);
        }
        unreachable!("retry loop returns or errors")
    }

    /// Lichess cloud-eval for a position (White POV); `None` on a cache miss.
    pub async fn cloud_eval(&self, fen: &str) -> Result<Option<CloudEvalResponse>> {
        let url = format!(
            "{LICHESS_BASE}/api/cloud-eval?fen={}&multiPv=6",
            urlencode(fen)
        );
        let res = send_logged(&self.http, self.http.get(&url)).await?;
        if res.status() == 404 {
            return Ok(None);
        }
        let status = res.status();
        if !status.is_success() {
            return Err(anyhow!("cloud-eval failed: {status}"));
        }
        let value: serde_json::Value = res.json().await?;
        if value.get("pvs").is_none() {
            return Ok(None);
        }
        Ok(Some(serde_json::from_value(value)?))
    }

    /// chess-api.com eval (White POV), the fallback when cloud-eval misses.
    /// Short timeout: in practice it either answers in under a second or hangs
    /// indefinitely; a hung fallback shouldn't stall the quiz move.
    pub async fn chess_api_eval(&self, fen: &str) -> Result<(Option<f64>, Option<i32>)> {
        let res = send_logged(
            &self.http,
            self.http
                .post(CHESS_API)
                .timeout(Duration::from_secs(5))
                .json(&serde_json::json!({ "fen": fen, "depth": 12 })),
        )
        .await?;
        let status = res.status();
        if !status.is_success() {
            return Err(anyhow!("chess-api.com failed: {status}"));
        }
        let value: serde_json::Value = res.json().await?;
        let pawns = value.get("eval").and_then(|v| v.as_f64());
        let centipawns = value.get("centipawns").and_then(|v| v.as_f64());
        let pawns = pawns.or_else(|| centipawns.map(|cp| cp / 100.0));
        let mate = value.get("mate").and_then(|v| v.as_i64()).map(|m| m as i32);
        Ok((pawns, mate))
    }

    /// Opening name from the masters explorer's `opening` field, or `None`.
    pub async fn opening_name(&self, fen: &str, token: Option<&str>) -> Result<Option<String>> {
        let res = self.masters(fen, token).await?;
        Ok(res
            .opening
            .and_then(|o| o.name)
            .filter(|n| !n.trim().is_empty()))
    }

    /// Authenticated user from a token. (Lichess's endpoint is /api/account;
    /// there is no /api/me.)
    pub async fn me(&self, token: &str) -> Result<LichessMe> {
        let url = format!("{LICHESS_BASE}/api/account");
        let value = self.get_json(&url, Some(token)).await?;
        Ok(serde_json::from_value(value)?)
    }

    /// Exchanges an OAuth authorization code for an access token (PKCE flow —
    /// Lichess has no app registration; the client_id is any stable string and
    /// there is no client secret).
    pub async fn exchange_token(
        &self,
        code: &str,
        client_id: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<String> {
        let token_req =
            self.http
                .post(format!("{LICHESS_BASE}/api/token"))
                .json(&serde_json::json!({
                    "grant_type": "authorization_code",
                    "code": code,
                    "client_id": client_id,
                    "code_verifier": code_verifier,
                    "redirect_uri": redirect_uri,
                }));
        let res = send_logged(&self.http, token_req).await?;
        let status = res.status();
        let value: serde_json::Value = res.json().await?;
        if !status.is_success() {
            return Err(anyhow!("token exchange failed: {status}: {value}"));
        }
        value
            .get("access_token")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| anyhow!("token exchange response missing access_token"))
    }

    /// Masters statistics for the played move (matched on SAN or castling-tolerant
    /// UCI) plus the top moves, in the contract's shape.
    pub fn masters_stats(
        &self,
        res: &ExplorerResponse,
        san: &str,
        played_uci: &str,
    ) -> MastersStats {
        let position_total = res.white + res.draws + res.black;
        if position_total == 0 {
            return MastersStats {
                played_percent: None,
                position_total: 0,
                top_moves: Vec::new(),
            };
        }
        let to_move = |m: &ExplorerMove| {
            let total = m.white + m.draws + m.black;
            MastersMove {
                san: m.san.clone(),
                percent: total as f64 / position_total as f64,
            }
        };
        // The explorer encodes castling as king-takes-rook UCI (e1h1); our moves use
        // the king-two-squares form (e1g1), so match on SAN (canonical for both).
        let played = res
            .moves
            .iter()
            .find(|m| m.san == san || m.uci == played_uci);
        let played_percent = played.map(|m| {
            let total = m.white + m.draws + m.black;
            total as f64 / position_total as f64
        });
        let mut top_moves: Vec<MastersMove> = res.moves.iter().map(to_move).collect();
        top_moves.sort_by(|a, b| {
            b.percent
                .partial_cmp(&a.percent)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        top_moves.truncate(5);
        MastersStats {
            played_percent,
            position_total,
            top_moves,
        }
    }
}

/// All 'YYYY-MM' months from 2019-01 through the month six months ago (the
/// players DB only serves settled history).
pub fn candidate_months() -> Vec<String> {
    let now = Utc::now();
    // End at the current month minus 6 (handles year wraparound).
    let total_months = (now.year() - 2019) * 12 + now.month() as i32 - 1 - 6;
    let mut out = Vec::new();
    for i in 0..total_months.max(0) {
        let year = 2019 + (i / 12);
        let month = (i % 12) + 1;
        out.push(format!("{year}-{month:02}"));
    }
    out
}

/// Comma-separated explorer rating buckets (of 100) covering [min, max].
pub fn rating_buckets(min_rating: i64, max_rating: i64) -> String {
    let lo = (min_rating / 100).max(0) * 100;
    let hi = (max_rating / 100).max(lo / 100) * 100;
    let mut buckets: Vec<String> = Vec::new();
    let mut b = lo;
    while b <= hi {
        buckets.push(b.to_string());
        b += 100;
    }
    buckets.join(",")
}

fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_months_spans_2019_to_six_months_ago() {
        let months = candidate_months();
        assert_eq!(months.first().unwrap(), "2019-01");
        assert_eq!(months.len(), expected_count());
        // Sorted ascending (rotation happens later, at query time).
        let mut sorted = months.clone();
        sorted.sort();
        assert_eq!(months, sorted);
    }

    fn expected_count() -> usize {
        let now = Utc::now();
        let months_since = (now.year() - 2019) * 12 + now.month() as i32 - 1 - 6;
        months_since.max(0) as usize
    }

    #[test]
    fn rating_buckets_cover_the_range() {
        assert_eq!(rating_buckets(1600, 2000), "1600,1700,1800,1900,2000");
        assert_eq!(rating_buckets(1650, 1699), "1600");
        assert_eq!(rating_buckets(0, 0), "0");
    }

    #[test]
    fn masters_stats_match_by_san_not_raw_castling_uci() {
        let res = ExplorerResponse {
            white: 750,
            draws: 200,
            black: 50,
            moves: vec![
                ExplorerMove {
                    uci: "e1h1".into(),
                    san: "O-O".into(),
                    white: 750,
                    draws: 200,
                    black: 50,
                },
                ExplorerMove {
                    uci: "g1f3".into(),
                    san: "Nf3".into(),
                    white: 100,
                    draws: 10,
                    black: 5,
                },
            ],
            top_games: vec![],
            recent_games: vec![],
            opening: Some(OpeningInfo {
                eco: Some("C60".into()),
                name: Some("Ruy Lopez".into()),
            }),
        };
        let lichess = Lichess::new();
        let stats = lichess.masters_stats(&res, "O-O", "e1g1");
        assert_eq!(stats.position_total, 1000);
        assert_eq!(stats.played_percent, Some(1.0));
        assert_eq!(stats.top_moves.len(), 2);
        assert_eq!(stats.top_moves[0].san, "O-O");
        assert_eq!(stats.top_moves[0].percent, 1.0);
        assert_eq!(stats.top_moves[1].percent, 0.115);
    }

    #[test]
    fn explorer_json_parses_with_missing_optional_fields() {
        let json = serde_json::json!({
            "white": 1, "draws": 0, "black": 1,
            "moves": [{ "uci": "e2e4", "san": "e4", "white": 1, "draws": 0, "black": 1 }],
            "recentGames": [{ "id": "abc" }],
            "opening": null
        });
        let res: ExplorerResponse = serde_json::from_value(json).unwrap();
        assert_eq!(res.moves.len(), 1);
        assert_eq!(res.recent_games[0].id, "abc");
        assert!(res.opening.is_none());
    }
}
