use serde::Deserialize;

/// Web Config
#[derive(Deserialize, Clone)]
pub struct WebConfig {
    pub addr: String,
}

/// Postgre Config
#[derive(Deserialize, Clone, Debug, Default)]
pub struct PostgreConfig {
    pub url: Option<String>,
    pub user: Option<String>,
    pub password: Option<String>,
    pub dbname: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
}

/// App Config
#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub web: WebConfig,
    pub pg: PostgreConfig,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, config::ConfigError> {
        let cfg = config::Config::builder();
        cfg.add_source(config::Environment::default())
            .build()?
            .try_deserialize()
    }
}
