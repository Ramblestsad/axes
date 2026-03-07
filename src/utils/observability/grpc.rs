use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use axum::http::{HeaderMap, Request, Response};
use tonic::Code;
use tower::{Layer, Service};
use tracing::Instrument;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use super::{metrics::record_grpc_metrics, tracing::attach_parent_context_from_headers};

#[derive(Clone, Copy, Default)]
pub struct GrpcObservabilityLayer;

pub fn grpc_observability_layer() -> GrpcObservabilityLayer {
    GrpcObservabilityLayer
}

impl<S> Layer<S> for GrpcObservabilityLayer {
    type Service = GrpcObservabilityService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        GrpcObservabilityService { inner }
    }
}

#[derive(Clone)]
pub struct GrpcObservabilityService<S> {
    inner: S,
}

impl<S, B, ResBody> Service<Request<B>> for GrpcObservabilityService<S>
where
    S: Service<Request<B>, Response = Response<ResBody>> + Send + 'static,
    S::Future: Send + 'static,
    ResBody: Send + 'static,
    S::Error: Send + 'static,
    B: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let (service, method) = parse_grpc_path(req.uri().path());
        let span_name = format!("{service}/{method}");
        let span = tracing::info_span!(
            "grpc.request",
            otel.name = %span_name,
            otel.kind = "server",
            rpc.system = "grpc",
            rpc.service = %service,
            rpc.method = %method
        );

        attach_parent_context_from_headers(&span, req.headers());

        let started_at = Instant::now();
        let fut = self.inner.call(req);
        let completion_span = span.clone();

        Box::pin(
            async move {
                let result = fut.await;
                let elapsed = started_at.elapsed();

                match &result {
                    Ok(response) => {
                        let code = grpc_status_from_headers(response.headers()).unwrap_or(Code::Ok);
                        record_completion(
                            &completion_span,
                            &service,
                            &method,
                            code,
                            elapsed.as_secs_f64(),
                        );
                    }
                    Err(_) => {
                        record_completion(
                            &completion_span,
                            &service,
                            &method,
                            Code::Unknown,
                            elapsed.as_secs_f64(),
                        );
                    }
                }

                result
            }
            .instrument(span),
        )
    }
}

fn parse_grpc_path(path: &str) -> (String, String) {
    let trimmed = path.trim_start_matches('/');
    match trimmed.split_once('/') {
        Some((service, method)) => (service.to_string(), method.to_string()),
        None => ("unknown".to_string(), trimmed.to_string()),
    }
}

fn grpc_status_from_headers(headers: &HeaderMap) -> Option<Code> {
    headers
        .get(tonic::Status::GRPC_STATUS)
        .map(|value| Code::from_bytes(value.as_ref()))
}

fn record_completion(
    span: &tracing::Span,
    service: &str,
    method: &str,
    code: Code,
    elapsed_seconds: f64,
) {
    span.set_attribute("rpc.grpc.status_code", code.to_string());
    if code != Code::Ok {
        span.set_attribute("otel.status_code", "ERROR");
    }

    record_grpc_metrics(service, method, code, elapsed_seconds);
    tracing::info!(
        parent: span,
        rpc.grpc.status_code = code.to_string(),
        duration_ms = elapsed_seconds * 1000.0,
        "grpc request completed"
    );
}
