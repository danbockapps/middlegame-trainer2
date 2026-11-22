use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub client_id: Option<String>,
    pub redirect_uri: String,
    pub bind: String,
    pub database_url: String,
    pub static_dir: Option<PathBuf>,
}

impl Config {
    /// Reads configuration from the environment. Missing OAuth credentials are not
    /// fatal: the app compiles and runs, and auth endpoints report "not_configured".
    pub fn from_env() -> Self {
        let client_id = non_empty_env("LICHESS_CLIENT_ID");
        let redirect_uri = std::env::var("LICHESS_REDIRECT_URI")
            .unwrap_or_else(|_| "http://localhost:8080/api/auth/callback".to_string());
        let bind = std::env::var("BIND").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data/app.db".to_string());
        let static_dir = non_empty_env("STATIC_DIR").map(PathBuf::from);
        Self {
            client_id,
            redirect_uri,
            bind,
            database_url,
            static_dir,
        }
    }

    pub fn oauth_configured(&self) -> bool {
        self.client_id.is_some()
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}
