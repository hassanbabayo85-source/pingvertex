use std::env;

/// Application configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub jwt_secret: String,
}

impl Config {
    pub fn from_env() -> Self {
        dotenvy::dotenv().ok();

        Self {
            database_url: env::var("DATABASE_URL")
                .expect("DATABASE_URL environment variable is not set"),

            jwt_secret: env::var("JWT_SECRET")
                .unwrap_or_else(|_| "development-secret-change-me".to_string()),
        }
    }
}
