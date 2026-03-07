use std::time::Instant;

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{metrics::record_http_metrics, tracing::attach_parent_context_from_headers};

pub async fn http_observability(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri_path = req.uri().path().to_string();
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(MatchedPath::as_str)
        .unwrap_or(uri_path.as_str())
        .to_string();
    let span_name = format!("{method} {route}");

    let span = tracing::info_span!(
        "http.request",
        otel.name = %span_name,
        otel.kind = "server",
        http.method = %method,
        http.route = %route,
        url.path = %uri_path
    );

    attach_parent_context_from_headers(&span, req.headers());

    let started_at = Instant::now();
    let response = next.run(req).instrument(span.clone()).await;
    let status = response.status();
    let elapsed = started_at.elapsed();

    span.set_attribute("http.response.status_code", i64::from(status.as_u16()));
    if status.is_server_error() {
        span.set_attribute("otel.status_code", "ERROR");
    }

    record_http_metrics(&method.to_string(), &route, status.as_u16(), elapsed.as_secs_f64());
    tracing::info!(
        parent: &span,
        http.status_code = status.as_u16(),
        duration_ms = elapsed.as_secs_f64() * 1000.0,
        "http request completed"
    );

    response
}
