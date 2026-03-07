use std::env;

use opentelemetry::KeyValue;
use opentelemetry_sdk::Resource;

use super::otlp::{OtlpProtocol, parse_otlp_headers};

#[derive(Clone)]
pub(super) struct ObservabilitySettings {
    pub(super) environment: String,
    pub(super) service_name: String,
    pub(super) service_version: String,
    pub(super) otlp_endpoint: Option<String>,
    pub(super) otlp_protocol: OtlpProtocol,
    pub(super) otlp_headers: Vec<(String, String)>,
    pub(super) trace_sampler: String,
    pub(super) trace_sampler_arg: f64,
}

impl ObservabilitySettings {
    pub(super) fn from_env() -> Self {
        let environment = env::var("ENVIRONMENT")
            .unwrap_or_else(|_| "development".to_string())
            .trim()
            .to_ascii_lowercase();

        Self {
            environment: environment.clone(),
            service_name: env::var("OTEL_SERVICE_NAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| env!("CARGO_PKG_NAME").to_string()),
            service_version: env!("CARGO_PKG_VERSION").to_string(),
            otlp_endpoint: env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            otlp_protocol: OtlpProtocol::parse(env::var("OTEL_EXPORTER_OTLP_PROTOCOL").ok()),
            otlp_headers: parse_otlp_headers(env::var("OTEL_EXPORTER_OTLP_HEADERS").ok()),
            trace_sampler: env::var("OTEL_TRACES_SAMPLER")
                .unwrap_or_else(|_| "parentbased_traceidratio".to_string()),
            trace_sampler_arg: parse_trace_sampler_arg(env::var("OTEL_TRACES_SAMPLER_ARG").ok()),
        }
    }

    pub(super) fn should_enable_exporters(&self) -> bool {
        self.otlp_endpoint.is_some()
    }

    pub(super) fn resource(&self) -> Resource {
        Resource::builder()
            .with_service_name(self.service_name.clone())
            .with_attributes([
                KeyValue::new("service.version", self.service_version.clone()),
                KeyValue::new("deployment.environment", self.environment.clone()),
            ])
            .build()
    }
}

fn parse_trace_sampler_arg(value: Option<String>) -> f64 {
    value
        .as_deref()
        .and_then(|raw| raw.trim().parse::<f64>().ok())
        .map(|ratio| ratio.clamp(0.0, 1.0))
        .unwrap_or(1.0)
}

#[cfg(test)]
mod tests {
    use super::{ObservabilitySettings, parse_trace_sampler_arg};
    use crate::utils::observability::otlp::OtlpProtocol;

    fn make_settings(environment: &str, endpoint: Option<&str>) -> ObservabilitySettings {
        ObservabilitySettings {
            environment: environment.to_string(),
            service_name: "axes".to_string(),
            service_version: "0.1.0".to_string(),
            otlp_endpoint: endpoint.map(str::to_string),
            otlp_protocol: OtlpProtocol::Grpc,
            otlp_headers: Vec::new(),
            trace_sampler: "parentbased_traceidratio".to_string(),
            trace_sampler_arg: 1.0,
        }
    }

    #[test]
    fn enables_exporters_when_endpoint_exists_even_in_development() {
        let settings = make_settings("development", Some("http://localhost:4317"));

        assert!(settings.should_enable_exporters());
    }

    #[test]
    fn disables_exporters_when_endpoint_is_missing() {
        let settings = make_settings("production", None);

        assert!(!settings.should_enable_exporters());
    }

    #[test]
    fn clamps_trace_sampler_ratio() {
        assert_eq!(parse_trace_sampler_arg(Some("2.5".to_string())), 1.0);
        assert_eq!(parse_trace_sampler_arg(Some("-1".to_string())), 0.0);
    }
}
