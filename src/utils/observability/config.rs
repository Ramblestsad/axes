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

    pub(super) fn is_development(&self) -> bool {
        self.environment == "development"
    }

    pub(super) fn should_enable_exporters(&self) -> bool {
        !self.is_development() && self.otlp_endpoint.is_some()
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
