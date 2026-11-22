use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub struct OpeningListItem {
    pub id: i64,
    pub name: Option<String>,
    pub side: String,
    #[serde(rename = "nameOverridden")]
    pub name_overridden: bool,
    #[serde(rename = "rangeMin")]
    pub range_min: i64,
    #[serde(rename = "rangeMax")]
    pub range_max: i64,
    #[serde(rename = "moveCount")]
    pub move_count: i64,
}

#[derive(Debug, Serialize)]
pub struct OpeningDetail {
    #[serde(flatten)]
    pub item: OpeningListItem,
    pub moves: Vec<String>,
    pub deleted: bool,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub id: String,
    pub username: String,
    pub ratings: RatingsResponse,
}

#[derive(Debug, Serialize)]
pub struct RatingsResponse {
    pub blitz: Option<i64>,
    pub rapid: Option<i64>,
    pub classical: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeControl {
    Blitz,
    Rapid,
    Classical,
}

impl TimeControl {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeControl::Blitz => "blitz",
            TimeControl::Rapid => "rapid",
            TimeControl::Classical => "classical",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SearchSettings {
    #[serde(rename = "timeControl")]
    pub time_control: &'static str,
    #[serde(rename = "minRating")]
    pub min_rating: i64,
    #[serde(rename = "maxRating")]
    pub max_rating: i64,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSettings {
    #[serde(rename = "timeControl")]
    pub time_control: String,
    #[serde(rename = "minRating")]
    pub min_rating: i64,
    #[serde(rename = "maxRating")]
    pub max_rating: i64,
}

#[derive(Debug, Serialize)]
pub struct QuizPosition {
    pub fen: String,
    #[serde(rename = "moveNumber")]
    pub move_number: u32,
    pub side: String,
    #[serde(rename = "gameId")]
    pub game_id: String,
    #[serde(rename = "gameUrl")]
    pub game_url: String,
}

/// Response of `POST /api/quiz/start` and `POST /api/quiz/:id/skip`.
#[derive(Debug, Serialize)]
pub struct QuizStart {
    #[serde(rename = "quizId")]
    pub quiz_id: String,
    pub position: QuizPosition,
}

/// `0.31` for pawns, or `{"mate": 3}` for mating lines. White-POV.
/// One eval number. `Unavailable` serializes as JSON `null` (the frontend renders
/// "eval unavailable" for it).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(untagged)]
pub enum EvalValue {
    Pawns(f64),
    Mate { mate: i32 },
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct EvalDelta {
    pub before: EvalValue,
    pub after: EvalValue,
    pub source: &'static str,
}

#[derive(Debug, Serialize)]
pub struct MastersMove {
    pub san: String,
    pub percent: f64,
}

#[derive(Debug, Serialize)]
pub struct MastersStats {
    #[serde(rename = "playedPercent")]
    pub played_percent: Option<f64>,
    #[serde(rename = "positionTotal")]
    pub position_total: i64,
    #[serde(rename = "topMoves")]
    pub top_moves: Vec<MastersMove>,
}

#[derive(Debug, Serialize)]
pub struct MoveResult {
    pub masters: MastersStats,
    pub eval: EvalDelta,
}
