use std::sync::OnceLock;

use opentelemetry::{KeyValue, global};
use opentelemetry_sdk::metrics::{PeriodicReader, SdkMeterProvider};
use tonic::Code;

use super::{config::ObservabilitySettings, otlp::build_metric_exporter};

static METRICS: OnceLock<MetricsInstruments> = OnceLock::new();

struct MetricsInstruments {
    http_requests_total: opentelemetry::metrics::Counter<u64>,
    http_request_duration_seconds: opentelemetry::metrics::Histogram<f64>,
    grpc_requests_total: opentelemetry::metrics::Counter<u64>,
    grpc_request_duration_seconds: opentelemetry::metrics::Histogram<f64>,
}

impl MetricsInstruments {
    fn new() -> Self {
        let meter = global::meter("axes");

        Self {
            http_requests_total: meter
                .u64_counter("http.server.request.count")
                .with_description("Total number of HTTP requests handled by axes.")
                .build(),
            http_request_duration_seconds: meter
                .f64_histogram("http.server.request.duration")
                .with_description("HTTP request latency in seconds.")
                .with_unit("s")
                .build(),
            grpc_requests_total: meter
                .u64_counter("rpc.server.request.count")
                .with_description("Total number of gRPC requests handled by axes.")
                .build(),
            grpc_request_duration_seconds: meter
                .f64_histogram("rpc.server.request.duration")
                .with_description("gRPC request latency in seconds.")
                .with_unit("s")
                .build(),
        }
    }
}

pub(super) fn init_metrics() {
    let _ = METRICS.set(MetricsInstruments::new());
}

pub(super) fn build_meter_provider(
    settings: &ObservabilitySettings,
    warnings: &mut Vec<String>,
) -> Option<SdkMeterProvider> {
    let endpoint = settings.otlp_endpoint.as_ref()?;

    match build_metric_exporter(settings.otlp_protocol, endpoint, &settings.otlp_headers) {
        Ok(exporter) => Some(
            SdkMeterProvider::builder()
                .with_resource(settings.resource())
                .with_reader(PeriodicReader::builder(exporter).build())
                .build(),
        ),
        Err(error) => {
            warnings.push(format!("metric exporter disabled: {error:#}"));
            None
        }
    }
}

pub(crate) fn record_http_metrics(
    method: &str,
    route: &str,
    status_code: u16,
    elapsed_seconds: f64,
) {
    let Some(metrics) = METRICS.get() else {
        return;
    };

    let attributes = [
        KeyValue::new("http.request.method", method.to_string()),
        KeyValue::new("http.route", route.to_string()),
        KeyValue::new("http.response.status_code", i64::from(status_code)),
    ];

    metrics.http_requests_total.add(1, &attributes);
    metrics
        .http_request_duration_seconds
        .record(elapsed_seconds, &attributes);
}

pub(crate) fn record_grpc_metrics(service: &str, method: &str, code: Code, elapsed_seconds: f64) {
    let Some(metrics) = METRICS.get() else {
        return;
    };

    let attributes = [
        KeyValue::new("rpc.system", "grpc"),
        KeyValue::new("rpc.service", service.to_string()),
        KeyValue::new("rpc.method", method.to_string()),
        KeyValue::new("rpc.grpc.status_code", code.to_string()),
    ];

    metrics.grpc_requests_total.add(1, &attributes);
    metrics
        .grpc_request_duration_seconds
        .record(elapsed_seconds, &attributes);
}
