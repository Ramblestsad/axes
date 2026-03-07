use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use axum::http::{HeaderMap, Request, Response};
use http_body::{Body, Frame, SizeHint};
use tonic::Code;
use tower::{Layer, Service};
use tracing::Instrument;

use super::{
    metrics::record_grpc_metrics,
    tracing::{attach_parent_context_from_headers, set_span_status},
};

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
    ResBody: Body + Send + Unpin + 'static,
    S::Error: Send + 'static,
    B: Send + 'static,
{
    type Response = Response<GrpcObservabilityBody<ResBody>>;
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

                match result {
                    Ok(response) => {
                        let fallback_code = grpc_status_from_parts(Some(response.headers()), None);
                        let (parts, body) = response.into_parts();
                        let body = GrpcObservabilityBody::new(
                            body,
                            completion_span,
                            service,
                            method,
                            started_at,
                            fallback_code,
                        );
                        Ok(Response::from_parts(parts, body))
                    }
                    Err(error) => {
                        record_completion(
                            &completion_span,
                            &service,
                            &method,
                            Code::Unknown,
                            started_at.elapsed().as_secs_f64(),
                        );
                        Err(error)
                    }
                }
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

fn grpc_status_from_parts(
    headers: Option<&HeaderMap>,
    trailers: Option<&HeaderMap>,
) -> Option<Code> {
    trailers
        .and_then(grpc_status_from_headers)
        .or_else(|| headers.and_then(grpc_status_from_headers))
}

pub struct GrpcObservabilityBody<B> {
    inner: B,
    span: tracing::Span,
    service: String,
    method: String,
    started_at: Instant,
    fallback_code: Option<Code>,
    completed: bool,
}

impl<B> GrpcObservabilityBody<B> {
    fn new(
        inner: B,
        span: tracing::Span,
        service: String,
        method: String,
        started_at: Instant,
        fallback_code: Option<Code>,
    ) -> Self {
        Self { inner, span, service, method, started_at, fallback_code, completed: false }
    }

    fn complete(&mut self, code: Code) {
        if self.completed {
            return;
        }

        self.completed = true;
        record_completion(
            &self.span,
            &self.service,
            &self.method,
            code,
            self.started_at.elapsed().as_secs_f64(),
        );
    }
}

impl<B> Body for GrpcObservabilityBody<B>
where
    B: Body + Unpin,
{
    type Data = B::Data;
    type Error = B::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.as_mut().get_mut();

        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(code) =
                    grpc_status_from_parts(None, frame.trailers_ref()).or(this.fallback_code)
                {
                    this.complete(code);
                }

                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => {
                this.complete(Code::Unknown);
                Poll::Ready(Some(Err(error)))
            }
            Poll::Ready(None) => {
                let code = this.fallback_code.unwrap_or(Code::Ok);
                this.complete(code);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

fn record_completion(
    span: &tracing::Span,
    service: &str,
    method: &str,
    code: Code,
    elapsed_seconds: f64,
) {
    let code_text = code.to_string();
    set_span_status(span, "rpc.grpc.status_code", code_text.clone(), code != Code::Ok);

    record_grpc_metrics(service, method, code, elapsed_seconds);
    tracing::info!(
        parent: span,
        rpc.grpc.status_code = code_text,
        duration_ms = elapsed_seconds * 1000.0,
        "grpc request completed"
    );
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use tonic::Code;

    use super::{grpc_status_from_parts, parse_grpc_path};

    #[test]
    fn parses_grpc_service_and_method_from_path() {
        let (service, method) = parse_grpc_path("/greeter.v1.Greeter/SayHello");

        assert_eq!(service, "greeter.v1.Greeter");
        assert_eq!(method, "SayHello");
    }

    #[test]
    fn prefers_trailer_status_over_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(tonic::Status::GRPC_STATUS, "0".parse().unwrap());

        let mut trailers = HeaderMap::new();
        trailers.insert(tonic::Status::GRPC_STATUS, "14".parse().unwrap());

        assert_eq!(
            grpc_status_from_parts(Some(&headers), Some(&trailers)),
            Some(Code::Unavailable)
        );
    }

    #[test]
    fn falls_back_to_headers_when_trailers_are_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(tonic::Status::GRPC_STATUS, "7".parse().unwrap());

        assert_eq!(grpc_status_from_parts(Some(&headers), None), Some(Code::PermissionDenied));
    }
}
