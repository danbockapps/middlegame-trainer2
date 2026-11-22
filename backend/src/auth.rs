use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;
use sqlx::Row;

use crate::error::{ApiError, ApiResult};
use crate::lichess::LichessMe;
use crate::models::{MeResponse, RatingsResponse};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "sid";
const OAUTH_STATE_COOKIE: &str = "oauth_state";
const OAUTH_VERIFIER_COOKIE: &str = "oauth_verifier";
const OAUTH_NEXT_COOKIE: &str = "oauth_next";

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub token: String,
    pub blitz: Option<i64>,
    pub rapid: Option<i64>,
    pub classical: Option<i64>,
}

/// Resolves the logged-in user from the `sid` cookie.
pub async fn user_from_jar(state: &AppState, jar: &CookieJar) -> ApiResult<User> {
    let sid = jar
        .get(SESSION_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| ApiError::unauthorized("not logged in"))?;
    user_from_sid(state, &sid).await
}

pub async fn user_from_sid(state: &AppState, sid: &str) -> ApiResult<User> {
    let row = sqlx::query(
        r#"
        SELECT u.id, u.username, u.token, u.rating_blitz, u.rating_rapid, u.rating_classical
        FROM sessions s JOIN users u ON u.id = s.user_id
        WHERE s.sid = ?
        "#,
    )
    .bind(sid)
    .fetch_optional(&*state.pool)
    .await?
    .ok_or_else(|| ApiError::unauthorized("session expired or invalid"))?;
    Ok(User {
        id: row.get("id"),
        username: row.get("username"),
        token: row.get("token"),
        blitz: row.get("rating_blitz"),
        rapid: row.get("rating_rapid"),
        classical: row.get("rating_classical"),
    })
}

pub fn ratings_response(user: &User) -> RatingsResponse {
    RatingsResponse {
        blitz: user.blitz,
        rapid: user.rapid,
        classical: user.classical,
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginQuery {
    #[serde(default)]
    pub next: Option<String>,
}

pub async fn login(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<LoginQuery>,
) -> ApiResult<Response> {
    if !state.cfg.oauth_configured() {
        return Err(ApiError::not_configured(
            "LICHESS_CLIENT_ID is not set (Lichess has no app registration; any stable string works)",
        ));
    }
    let state_value = uuid::Uuid::new_v4().to_string();
    // PKCE: Lichess has no client secrets; we prove possession of the code via
    // a random verifier whose S256 hash was sent in the authorize request.
    let verifier = pkce_verifier();
    let challenge = pkce_challenge(&verifier);
    let next = query.next.unwrap_or_else(|| "/".to_string());
    let redirect = format!(
        "https://lichess.org/oauth?response_type=code&client_id={}&redirect_uri={}&scope=&state={}&code_challenge_method=S256&code_challenge={}",
        urlencode(state.cfg.client_id.as_deref().unwrap_or_default()),
        urlencode(&state.cfg.redirect_uri),
        urlencode(&state_value),
        urlencode(&challenge),
    );
    Ok((
        jar.add(
            Cookie::build((OAUTH_STATE_COOKIE, state_value))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::hours(1)),
        )
        .add(
            Cookie::build((OAUTH_VERIFIER_COOKIE, verifier))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::hours(1)),
        )
        .add(
            Cookie::build((OAUTH_NEXT_COOKIE, next))
                .path("/")
                .http_only(true)
                .same_site(SameSite::Lax)
                .max_age(time::Duration::hours(1)),
        ),
        Redirect::to(&redirect),
    )
        .into_response())
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
    #[serde(default)]
    pub error: Option<String>,
}

pub async fn callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(query): Query<CallbackQuery>,
) -> ApiResult<Response> {
    if let Some(error) = query.error {
        return Err(ApiError::bad_request(format!("oauth error: {error}")));
    }
    let expected_state = jar.get(OAUTH_STATE_COOKIE).map(|c| c.value().to_string());
    if expected_state.as_deref() != Some(query.state.as_str()) {
        return Err(ApiError::bad_request("oauth state mismatch"));
    }
    let verifier = jar
        .get(OAUTH_VERIFIER_COOKIE)
        .map(|c| c.value().to_string())
        .ok_or_else(|| ApiError::bad_request("oauth verifier cookie missing"))?;
    let next = jar
        .get(OAUTH_NEXT_COOKIE)
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| "/".to_string());

    let client_id = state
        .cfg
        .client_id
        .clone()
        .ok_or_else(|| ApiError::not_configured("LICHESS_CLIENT_ID is not set"))?;

    let token = state
        .lichess
        .exchange_token(&query.code, &client_id, &verifier, &state.cfg.redirect_uri)
        .await
        .map_err(|e| ApiError::internal(format!("token exchange failed: {e}")))?;
    let me: LichessMe = state
        .lichess
        .me(&token)
        .await
        .map_err(|e| ApiError::internal(format!("lichess /api/me failed: {e}")))?;

    let ratings = extract_ratings(&me);
    upsert_user(&state, &me, &token, &ratings).await?;
    if is_new_user(&state, &me.id).await? {
        seed_settings(&state, &me.id, &ratings).await?;
    }

    let sid = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (sid, user_id) VALUES (?, ?)")
        .bind(&sid)
        .bind(&me.id)
        .execute(&*state.pool)
        .await?;

    let cookie = Cookie::build((SESSION_COOKIE, sid))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(30));
    Ok((
        jar.add(cookie)
            .remove(OAUTH_STATE_COOKIE)
            .remove(OAUTH_VERIFIER_COOKIE)
            .remove(OAUTH_NEXT_COOKIE),
        Redirect::to(&next),
    )
        .into_response())
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar) -> ApiResult<Response> {
    if let Some(sid) = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()) {
        sqlx::query("DELETE FROM sessions WHERE sid = ?")
            .bind(&sid)
            .execute(&*state.pool)
            .await?;
    }
    let cookie = Cookie::build((SESSION_COOKIE, String::new()))
        .path("/")
        .http_only(true)
        .max_age(time::Duration::ZERO);
    Ok((jar.remove(cookie), StatusCode::NO_CONTENT).into_response())
}

pub async fn me(
    State(state): State<AppState>,
    jar: CookieJar,
) -> ApiResult<axum::Json<MeResponse>> {
    let user = user_from_jar(&state, &jar).await?;
    let response = MeResponse {
        ratings: ratings_response(&user),
        id: user.id,
        username: user.username,
    };
    Ok(axum::Json(response))
}

fn extract_ratings(me: &LichessMe) -> (Option<i64>, Option<i64>, Option<i64>) {
    let perf = |name: &str| {
        me.perfs
            .as_ref()
            .and_then(|p| p.get(name))
            .and_then(|p| p.get("rating"))
            .and_then(|r| r.as_i64())
    };
    (perf("blitz"), perf("rapid"), perf("classical"))
}

async fn upsert_user(
    state: &AppState,
    me: &LichessMe,
    token: &str,
    ratings: &(Option<i64>, Option<i64>, Option<i64>),
) -> ApiResult<()> {
    sqlx::query(
        r#"
        INSERT INTO users (id, username, token, rating_blitz, rating_rapid, rating_classical)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            username = excluded.username,
            token = excluded.token,
            rating_blitz = excluded.rating_blitz,
            rating_rapid = excluded.rating_rapid,
            rating_classical = excluded.rating_classical
        "#,
    )
    .bind(&me.id)
    .bind(&me.username)
    .bind(token)
    .bind(ratings.0)
    .bind(ratings.1)
    .bind(ratings.2)
    .execute(&*state.pool)
    .await?;
    Ok(())
}

async fn is_new_user(state: &AppState, user_id: &str) -> ApiResult<bool> {
    let row = sqlx::query("SELECT COUNT(*) AS n FROM settings WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(&*state.pool)
        .await?;
    Ok(row.get::<i64, _>("n") == 0)
}

/// Seeds search settings at first login: the user's own rating in the default
/// time control (rapid, then blitz, then classical, then 1500) ±200.
async fn seed_settings(
    state: &AppState,
    user_id: &str,
    ratings: &(Option<i64>, Option<i64>, Option<i64>),
) -> ApiResult<()> {
    let base = ratings.1.or(ratings.0).or(ratings.2).unwrap_or(1500);
    let min = (base - 200).max(0);
    let max = base + 200;
    sqlx::query("INSERT INTO settings (user_id, min_rating, max_rating) VALUES (?, ?, ?)")
        .bind(user_id)
        .bind(min)
        .bind(max)
        .execute(&*state.pool)
        .await?;
    Ok(())
}

/// 32 random bytes, base64url-encoded — the PKCE code verifier.
fn pkce_verifier() -> String {
    use base64::Engine as _;
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// S256 PKCE challenge: base64url(SHA-256(verifier)), no padding.
fn pkce_challenge(verifier: &str) -> String {
    use base64::Engine as _;
    use sha2::Digest;
    let digest = sha2::Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
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
