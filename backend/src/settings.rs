use axum::Json;
use axum::extract::State;
use axum_extra::extract::CookieJar;

use crate::auth::user_from_jar;
use crate::error::{ApiError, ApiResult};
use crate::models::{SearchSettings, TimeControl, UpdateSettings};
use crate::state::AppState;
use sqlx::Row;

pub async fn get(State(state): State<AppState>, jar: CookieJar) -> ApiResult<Json<SearchSettings>> {
    let user = user_from_jar(&state, &jar).await?;
    let row =
        sqlx::query("SELECT time_control, min_rating, max_rating FROM settings WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&*state.pool)
            .await?;
    Ok(Json(SearchSettings {
        time_control: match row.get::<String, _>("time_control").as_str() {
            "blitz" => "blitz",
            "classical" => "classical",
            _ => "rapid",
        },
        min_rating: row.get("min_rating"),
        max_rating: row.get("max_rating"),
    }))
}

pub async fn put(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<UpdateSettings>,
) -> ApiResult<Json<SearchSettings>> {
    let user = user_from_jar(&state, &jar).await?;
    let time_control = match body.time_control.as_str() {
        "blitz" => TimeControl::Blitz,
        "rapid" => TimeControl::Rapid,
        "classical" => TimeControl::Classical,
        _ => {
            return Err(ApiError::bad_request(
                "timeControl must be blitz, rapid or classical",
            ));
        }
    };
    if body.min_rating < 0 || body.max_rating < body.min_rating {
        return Err(ApiError::bad_request(
            "ratings must satisfy 0 <= minRating <= maxRating",
        ));
    }
    sqlx::query(
        "UPDATE settings SET time_control = ?, min_rating = ?, max_rating = ? WHERE user_id = ?",
    )
    .bind(body.time_control)
    .bind(body.min_rating)
    .bind(body.max_rating)
    .bind(&user.id)
    .execute(&*state.pool)
    .await?;
    Ok(Json(SearchSettings {
        time_control: time_control.as_str(),
        min_rating: body.min_rating,
        max_rating: body.max_rating,
    }))
}
