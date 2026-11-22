use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use serde::Deserialize;
use sqlx::Row;

use crate::auth::user_from_jar;
use crate::error::{ApiError, ApiResult};
use crate::models::{OpeningDetail, OpeningListItem};
use crate::state::AppState;
use crate::tree::{build_tree, naming_fen};

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(rename = "deleted")]
    pub deleted: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateOpening {
    pub side: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateOpening {
    pub moves: Vec<String>,
    #[serde(rename = "nameOverride")]
    pub name_override: Option<String>,
    #[serde(rename = "rangeMin", default = "default_range_min")]
    pub range_min: i64,
    #[serde(rename = "rangeMax", default = "default_range_max")]
    pub range_max: i64,
}

fn default_range_min() -> i64 {
    0
}

fn default_range_max() -> i64 {
    2
}

struct OpeningRow {
    id: i64,
    name: Option<String>,
    side: String,
    name_overridden: bool,
    range_min: i64,
    range_max: i64,
    move_count: i64,
    deleted: bool,
    moves: Vec<String>,
}

fn move_count(moves: &[String]) -> i64 {
    moves.iter().filter(|m| *m != "-").count() as i64
}

async fn fetch_opening(state: &AppState, user_id: &str, id: i64) -> ApiResult<Option<OpeningRow>> {
    let row = sqlx::query(
        "SELECT id, name, name_overridden, side, moves, range_min, range_max, deleted_at FROM openings WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(&*state.pool)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let moves_json: String = row.get("moves");
    let moves: Vec<String> = serde_json::from_str(&moves_json).unwrap_or_default();
    Ok(Some(OpeningRow {
        id: row.get("id"),
        name: row.get("name"),
        side: row.get("side"),
        name_overridden: row.get::<i64, _>("name_overridden") != 0,
        range_min: row.get("range_min"),
        range_max: row.get("range_max"),
        move_count: move_count(&moves),
        deleted: row.get::<Option<String>, _>("deleted_at").is_some(),
        moves,
    }))
}

fn to_detail(row: &OpeningRow) -> OpeningDetail {
    OpeningDetail {
        item: OpeningListItem {
            id: row.id,
            name: row.name.clone(),
            side: row.side.clone(),
            name_overridden: row.name_overridden,
            range_min: row.range_min,
            range_max: row.range_max,
            move_count: row.move_count,
        },
        moves: row.moves.clone(),
        deleted: row.deleted,
    }
}

fn to_list_item(row: &OpeningRow) -> OpeningListItem {
    OpeningListItem {
        id: row.id,
        name: row.name.clone(),
        side: row.side.clone(),
        name_overridden: row.name_overridden,
        range_min: row.range_min,
        range_max: row.range_max,
        move_count: row.move_count,
    }
}

fn require_own(row: Option<OpeningRow>) -> ApiResult<OpeningRow> {
    // Not found vs. not-owned are intentionally indistinguishable.
    row.ok_or_else(|| ApiError::not_found("opening not found"))
}

pub async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<Json<Vec<OpeningListItem>>> {
    let user = user_from_jar(&state, &jar).await?;
    let deleted = query.deleted.as_deref() == Some("1");
    let sql = if deleted {
        "SELECT id, name, name_overridden, side, moves, range_min, range_max, deleted_at
         FROM openings WHERE user_id = ? AND deleted_at IS NOT NULL ORDER BY id DESC"
    } else {
        "SELECT id, name, name_overridden, side, moves, range_min, range_max, deleted_at
         FROM openings WHERE user_id = ? AND deleted_at IS NULL ORDER BY id DESC"
    };
    let rows = sqlx::query(sql)
        .bind(&user.id)
        .fetch_all(&*state.pool)
        .await?;
    let items: Vec<OpeningListItem> = rows
        .iter()
        .map(|r| {
            let moves_json: String = r.get::<String, _>("moves");
            let moves: Vec<String> = serde_json::from_str(&moves_json).unwrap_or_default();
            OpeningRow {
                id: r.get("id"),
                name: r.get("name"),
                side: r.get("side"),
                name_overridden: r.get::<i64, _>("name_overridden") != 0,
                range_min: r.get("range_min"),
                range_max: r.get("range_max"),
                move_count: move_count(&moves),
                deleted: r.get::<Option<String>, _>("deleted_at").is_some(),
                moves,
            }
        })
        .filter(|o| o.deleted == deleted)
        .map(|o| to_list_item(&o))
        .collect();
    Ok(Json(items))
}

pub async fn create(
    State(state): State<AppState>,
    jar: axum_extra::extract::CookieJar,
    Json(body): Json<CreateOpening>,
) -> ApiResult<(StatusCode, Json<OpeningDetail>)> {
    let user = user_from_jar(&state, &jar).await?;
    let side = match body.side.as_str() {
        "white" | "black" => body.side.clone(),
        _ => return Err(ApiError::bad_request("side must be \"white\" or \"black\"")),
    };
    let result = sqlx::query("INSERT INTO openings (user_id, side) VALUES (?, ?) RETURNING id")
        .bind(&user.id)
        .bind(&side)
        .fetch_one(&*state.pool)
        .await?;
    let id = result.get::<i64, _>("id");
    let row = require_own(fetch_opening(&state, &user.id, id).await?)?;
    Ok((StatusCode::CREATED, Json(to_detail(&row))))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<Json<OpeningDetail>> {
    let user = user_from_jar(&state, &jar).await?;
    let row = require_own(fetch_opening(&state, &user.id, id).await?)?;
    Ok(Json(to_detail(&row)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: axum_extra::extract::CookieJar,
    Json(body): Json<UpdateOpening>,
) -> ApiResult<Json<OpeningDetail>> {
    let user = user_from_jar(&state, &jar).await?;
    let existing = require_own(fetch_opening(&state, &user.id, id).await?)?;

    // Validate all moves are parseable UCI and the tree is well-formed.
    let tree = build_tree(&body.moves);
    let moves_json =
        serde_json::to_string(&body.moves).map_err(|e| ApiError::internal(e.to_string()))?;

    // Compute/refresh the name from the Lichess masters explorer unless overridden.
    let (name, name_overridden) = match body.name_override {
        Some(ref override_name) if !override_name.trim().is_empty() => {
            (Some(override_name.trim().to_string()), true)
        }
        _ => {
            if existing.name_overridden {
                (existing.name.clone(), true)
            } else {
                let fen = naming_fen(&tree);
                let name = state
                    .lichess
                    .opening_name(&fen, Some(&user.token))
                    .await
                    .map_err(|e| ApiError::internal(format!("masters lookup failed: {e}")))?;
                (name, false)
            }
        }
    };

    sqlx::query(
        "UPDATE openings SET moves = ?, name = ?, name_overridden = ?, range_min = ?, range_max = ?,
         updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?",
    )
    .bind(&moves_json)
    .bind(&name)
    .bind(i64::from(name_overridden))
    .bind(body.range_min)
    .bind(body.range_max)
    .bind(id)
    .execute(&*state.pool)
    .await?;

    let row = require_own(fetch_opening(&state, &user.id, id).await?)?;
    Ok(Json(to_detail(&row)))
}

pub async fn soft_delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<StatusCode> {
    let user = user_from_jar(&state, &jar).await?;
    require_own(fetch_opening(&state, &user.id, id).await?)?;
    sqlx::query(
        "UPDATE openings SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(&*state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn restore(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<Json<OpeningDetail>> {
    let user = user_from_jar(&state, &jar).await?;
    require_own(fetch_opening(&state, &user.id, id).await?)?;
    sqlx::query("UPDATE openings SET deleted_at = NULL WHERE id = ?")
        .bind(id)
        .execute(&*state.pool)
        .await?;
    let row = require_own(fetch_opening(&state, &user.id, id).await?)?;
    Ok(Json(to_detail(&row)))
}

pub async fn permanent_delete(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    jar: axum_extra::extract::CookieJar,
) -> ApiResult<Response> {
    let user = user_from_jar(&state, &jar).await?;
    let row = require_own(fetch_opening(&state, &user.id, id).await?)?;
    if !row.deleted {
        return Err(ApiError::conflict(
            "not_soft_deleted",
            "permanently deleting requires a prior soft delete",
        ));
    }
    sqlx::query("DELETE FROM openings WHERE id = ?")
        .bind(id)
        .execute(&*state.pool)
        .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
