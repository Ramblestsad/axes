mod config;
mod grpc;
mod http;
mod logging;
mod metrics;
mod otlp;
mod tracing;

use opentelemetry::{global, trace::TracerProvider as _};
use opentelemetry_sdk::{
    metrics::SdkMeterProvider, propagation::TraceContextPropagator, trace::SdkTracerProvider,
};

pub use grpc::grpc_observability_layer;
pub use http::http_observability;

pub struct ObservabilityGuard {
    tracer_provider: Option<SdkTracerProvider>,
    meter_provider: Option<SdkMeterProvider>,
}

impl ObservabilityGuard {
    pub fn shutdown(self) -> anyhow::Result<()> {
        if let Some(meter_provider) = self.meter_provider {
            meter_provider.shutdown()?;
        }

        if let Some(tracer_provider) = self.tracer_provider {
            tracer_provider.shutdown()?;
        }

        Ok(())
    }
}

pub fn init_observability() -> ObservabilityGuard {
    let settings = config::ObservabilitySettings::from_env();
    let mut warnings = Vec::new();
    let tracer_provider = if settings.is_development() {
        None
    } else {
        Some(tracing::build_tracer_provider(&settings, &mut warnings))
    };

    let tracer = tracer_provider
        .as_ref()
        .map(|provider| provider.tracer("axes"));
    let meter_provider = if settings.should_enable_exporters() {
        metrics::build_meter_provider(&settings, &mut warnings)
    } else {
        if !settings.is_development() && settings.otlp_endpoint.is_none() {
            warnings.push(
                "OTEL exporter disabled because OTEL_EXPORTER_OTLP_ENDPOINT is not configured"
                    .to_string(),
            );
        }
        None
    };

    global::set_text_map_propagator(TraceContextPropagator::new());
    if let Some(provider) = tracer_provider.as_ref() {
        global::set_tracer_provider(provider.clone());
    }

    if let Some(provider) = meter_provider.as_ref() {
        global::set_meter_provider(provider.clone());
        metrics::init_metrics();
    }

    logging::init_tracing_subscriber(&settings.environment, tracer);

    for warning in warnings {
        ::tracing::warn!(warning, "observability degraded");
    }

    ObservabilityGuard { tracer_provider, meter_provider }
}
