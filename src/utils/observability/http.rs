use std::time::Instant;

use axum::{
    extract::{MatchedPath, Request},
    middleware::Next,
    response::Response,
};
use tracing::Instrument;

use super::{
    metrics::record_http_metrics,
    tracing::{attach_parent_context_from_headers, set_span_status},
};

pub async fn http_observability(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
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

    set_span_status(
        &span,
        "http.response.status_code",
        i64::from(status.as_u16()),
        status.is_server_error(),
    );

    record_http_metrics(&method, &route, status.as_u16(), elapsed.as_secs_f64());
    tracing::info!(
        parent: &span,
        http.status_code = status.as_u16(),
        duration_ms = elapsed.as_secs_f64() * 1000.0,
        "http request completed"
    );

    response
}
