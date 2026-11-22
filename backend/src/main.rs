mod auth;
mod config;
mod db;
mod error;
mod eval;
mod game;
mod lichess;
mod models;
mod openings;
mod quiz;
mod settings;
mod state;
mod tree;

use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post};
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=info".into()),
        )
        .init();

    let cfg = Arc::new(config::Config::from_env());
    let pool = Arc::new(db::init(&cfg.database_url).await?);
    let app_state = AppState {
        cfg: cfg.clone(),
        pool,
        lichess: Arc::new(lichess::Lichess::new()),
    };

    if !cfg.oauth_configured() {
        tracing::warn!(
            "LICHESS_OAUTH_CLIENT_ID/SECRET not set: auth endpoints will return \"not_configured\" errors"
        );
    }

    let api = Router::new()
        .route("/auth/login", get(auth::login))
        .route("/auth/callback", get(auth::callback))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/openings", get(openings::list).post(openings::create))
        .route(
            "/openings/{id}",
            get(openings::get_one)
                .put(openings::update)
                .delete(openings::soft_delete),
        )
        .route("/openings/{id}/restore", post(openings::restore))
        .route(
            "/openings/{id}/permanent",
            delete(openings::permanent_delete),
        )
        .route("/quiz/start", post(quiz::start))
        .route("/quiz/{quiz_id}/move", post(quiz::play_move))
        .route("/quiz/{quiz_id}/skip", post(quiz::skip))
        .route("/settings", get(settings::get).put(settings::put))
        .with_state(app_state.clone());

    let router = Router::new()
        .nest("/api", api)
        .fallback_service(static_service(
            cfg.static_dir
                .as_deref()
                .unwrap_or(std::path::Path::new("static")),
        ))
        .layer(TraceLayer::new_for_http());
    if let Some(dir) = cfg.static_dir.as_ref().filter(|d| !d.is_dir()) {
        tracing::warn!(
            "STATIC_DIR {} does not exist; static routes will 404 until the frontend is built",
            dir.display()
        );
    }

    let listener = tokio::net::TcpListener::bind(&cfg.bind).await?;
    tracing::info!("listening on {}", cfg.bind);
    axum::serve(listener, router).await?;
    Ok(())
}

/// Static files for the SPA: serves `STATIC_DIR` (with index.html for `/`) and
/// falls back to `index.html` for unknown non-/api paths.
fn static_service(static_dir: &std::path::Path) -> ServeDir<ServeFile> {
    ServeDir::new(static_dir)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(static_dir.join("index.html")))
}
