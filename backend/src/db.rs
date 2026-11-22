use anyhow::Result;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

/// Creates the SQLite pool (and the database's parent directory) and runs the schema.
pub async fn init(database_url: &str) -> Result<SqlitePool> {
    if let Some(path) = database_url.strip_prefix("sqlite://") {
        let path = Path::new(path);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent).await?;
        }
    }

    let options = SqliteConnectOptions::from_str(database_url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(options)
        .await?;

    sqlx::query(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        "#,
    )
    .execute(&pool)
    .await?;

    for statement in SCHEMA.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        sqlx::query(statement).execute(&pool).await?;
    }

    Ok(pool)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    token TEXT NOT NULL,
    rating_blitz INTEGER,
    rating_rapid INTEGER,
    rating_classical INTEGER,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS sessions (
    sid TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS settings (
    user_id TEXT PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    time_control TEXT NOT NULL DEFAULT 'rapid'
        CHECK (time_control IN ('blitz', 'rapid', 'classical')),
    min_rating INTEGER NOT NULL,
    max_rating INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS openings (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT,
    name_overridden INTEGER NOT NULL DEFAULT 0,
    side TEXT NOT NULL CHECK (side IN ('white', 'black')),
    moves TEXT NOT NULL DEFAULT '[]',
    range_min INTEGER NOT NULL DEFAULT 0,
    range_max INTEGER NOT NULL DEFAULT 2,
    deleted_at TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS quiz_sessions (
    quiz_id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    opening_id INTEGER NOT NULL REFERENCES openings(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE IF NOT EXISTS quiz_positions (
    quiz_id TEXT PRIMARY KEY REFERENCES quiz_sessions(quiz_id) ON DELETE CASCADE,
    fen_before TEXT NOT NULL,
    game_id TEXT NOT NULL,
    move_number INTEGER NOT NULL,
    ply INTEGER NOT NULL,
    user_is_white INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_openings_user ON openings(user_id, deleted_at);
"#;
