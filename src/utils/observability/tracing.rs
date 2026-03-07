use axum::http::HeaderMap;
use opentelemetry::{
    global,
    trace::{SpanContext, TraceContextExt},
};
use opentelemetry_http::HeaderExtractor;
use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{config::ObservabilitySettings, otlp::build_span_exporter};

pub(super) fn build_tracer_provider(
    settings: &ObservabilitySettings,
    warnings: &mut Vec<String>,
) -> SdkTracerProvider {
    let builder = SdkTracerProvider::builder()
        .with_sampler(parse_sampler(&settings.trace_sampler, settings.trace_sampler_arg))
        .with_resource(settings.resource());

    match settings.otlp_endpoint.as_ref() {
        Some(endpoint) => {
            match build_span_exporter(settings.otlp_protocol, endpoint, &settings.otlp_headers) {
                Ok(exporter) => builder.with_batch_exporter(exporter).build(),
                Err(error) => {
                    warnings.push(format!("trace exporter disabled: {error:#}"));
                    builder.build()
                }
            }
        }
        None => builder.build(),
    }
}

pub(super) fn current_trace_context() -> Option<(String, String)> {
    let span_context = tracing::Span::current()
        .context()
        .span()
        .span_context()
        .clone();
    trace_and_span_id(&span_context)
}

pub(super) fn attach_parent_context_from_headers(span: &tracing::Span, headers: &HeaderMap) {
    let parent_context =
        global::get_text_map_propagator(|propagator| propagator.extract(&HeaderExtractor(headers)));
    let parent_span = parent_context.span();
    let parent_span_context = parent_span.span_context();

    if parent_span_context.is_valid() {
        let _ = span.set_parent(parent_context);
    }
}

fn parse_sampler(name: &str, ratio: f64) -> Sampler {
    match name.trim().to_ascii_lowercase().as_str() {
        "always_on" => Sampler::AlwaysOn,
        "always_off" => Sampler::AlwaysOff,
        "traceidratio" => Sampler::TraceIdRatioBased(ratio),
        "parentbased_always_on" => Sampler::ParentBased(Box::new(Sampler::AlwaysOn)),
        "parentbased_always_off" => Sampler::ParentBased(Box::new(Sampler::AlwaysOff)),
        _ => Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio))),
    }
}

fn trace_and_span_id(span_context: &SpanContext) -> Option<(String, String)> {
    if !span_context.is_valid() {
        return None;
    }

    Some((span_context.trace_id().to_string(), span_context.span_id().to_string()))
}
