use config::File;
use serde::Deserialize;

/// Postgre Config
#[derive(Deserialize, Clone, Debug, Default)]
pub struct PostgreConfig {
    pub write_url: Option<String>,
    pub read_url: Option<String>,
}

/// Redis Config
#[derive(Deserialize, Clone, Debug, Default)]
pub struct RedisConfig {
    pub url: Option<String>,
}

/// App Config
#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub pg: PostgreConfig,
    pub redis: RedisConfig,
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

impl PostgreConfig {
    pub fn required_write_url(&self) -> anyhow::Result<&str> {
        self.write_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Postgres write_url not found, check settings."))
    }

    pub fn required_read_url(&self) -> anyhow::Result<&str> {
        self.read_url
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("Postgres read_url not found, check settings."))
    }

    pub fn required_urls(&self) -> anyhow::Result<(&str, &str)> {
        let write_url = self.required_write_url()?;
        let read_url = self.required_read_url()?;

        Ok((write_url, read_url))
    }
}

#[cfg(test)]
mod tests {
    use super::{AppConfig, PostgreConfig};

    #[test]
    fn app_config_deserializes_read_write_postgres_urls() {
        let cfg = config::Config::builder()
            .set_override("pg.write_url", "postgresql://writer:5432/axes")
            .expect("write url should be set")
            .set_override("pg.read_url", "postgresql://reader:5433/axes")
            .expect("read url should be set")
            .set_override("redis.url", "redis://127.0.0.1:6379")
            .expect("redis url should be set")
            .build()
            .expect("config should build")
            .try_deserialize::<AppConfig>()
            .expect("config should deserialize");

        assert_eq!(cfg.pg.write_url.as_deref(), Some("postgresql://writer:5432/axes"));
        assert_eq!(cfg.pg.read_url.as_deref(), Some("postgresql://reader:5433/axes"));
    }

    #[test]
    fn postgres_config_requires_both_read_and_write_urls() {
        let cfg = PostgreConfig {
            write_url: Some("postgresql://writer:5432/axes".to_string()),
            read_url: None,
        };

        let error = cfg.required_urls().expect_err("missing read url should fail");
        assert!(error.to_string().contains("read"));
    }

    #[test]
    fn postgres_config_can_resolve_write_url_without_read_url() {
        let cfg = PostgreConfig {
            write_url: Some("postgresql://writer:5432/axes".to_string()),
            read_url: None,
        };

        assert_eq!(
            cfg.required_write_url().expect("write url should be available"),
            "postgresql://writer:5432/axes"
        );
    }
}
