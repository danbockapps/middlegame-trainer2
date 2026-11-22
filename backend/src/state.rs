use std::sync::Arc;

use sqlx::SqlitePool;

use crate::config::Config;
use crate::lichess::Lichess;

#[derive(Clone)]
pub struct AppState {
    pub cfg: Arc<Config>,
    pub pool: Arc<SqlitePool>,
    pub lichess: Arc<Lichess>,
}
