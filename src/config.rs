use config::File;
use serde::Deserialize;

/// Postgre Config
#[derive(Deserialize, Clone, Debug, Default)]
pub struct PostgreConfig {
    pub url: Option<String>,
}

/// App Config
#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub pg: PostgreConfig,
}

impl AppConfig {
    pub fn new() -> Result<Self, config::ConfigError> {
        let environment = std::env::var("ENVIRONMENT").unwrap_or_else(|_| "development".into());

        let cfg = config::Config::builder()
            .add_source(File::with_name(&format!("./settings/{}", environment)))
            .build()?;

        cfg.try_deserialize()
    }
}
