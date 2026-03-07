use std::collections::HashMap;

use anyhow::Context as _;
use opentelemetry_otlp::{
    MetricExporter, Protocol, SpanExporter, WithExportConfig, WithHttpConfig, WithTonicConfig,
};
use tonic::metadata::{Ascii, MetadataKey, MetadataValue};

#[derive(Clone, Copy)]
pub(super) enum OtlpProtocol {
    Grpc,
    HttpProtobuf,
}

impl OtlpProtocol {
    pub(super) fn parse(value: Option<String>) -> Self {
        match value
            .as_deref()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("http/protobuf") | Some("http_protobuf") => Self::HttpProtobuf,
            _ => Self::Grpc,
        }
    }
}

pub(super) fn build_span_exporter(
    protocol: OtlpProtocol,
    endpoint: &str,
    headers: &[(String, String)],
) -> anyhow::Result<SpanExporter> {
    match protocol {
        OtlpProtocol::Grpc => {
            let mut exporter = SpanExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.to_string());
            if !headers.is_empty() {
                exporter = exporter.with_metadata(to_metadata_map(headers)?);
            }
            exporter
                .build()
                .context("failed to build gRPC OTLP span exporter")
        }
        OtlpProtocol::HttpProtobuf => {
            let mut exporter = SpanExporter::builder()
                .with_http()
                .with_endpoint(endpoint.to_string())
                .with_protocol(Protocol::HttpBinary);
            if !headers.is_empty() {
                exporter = exporter.with_headers(to_headers_map(headers));
            }
            exporter
                .build()
                .context("failed to build HTTP OTLP span exporter")
        }
    }
}

pub(super) fn build_metric_exporter(
    protocol: OtlpProtocol,
    endpoint: &str,
    headers: &[(String, String)],
) -> anyhow::Result<MetricExporter> {
    match protocol {
        OtlpProtocol::Grpc => {
            let mut exporter = MetricExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.to_string());
            if !headers.is_empty() {
                exporter = exporter.with_metadata(to_metadata_map(headers)?);
            }
            exporter
                .build()
                .context("failed to build gRPC OTLP metric exporter")
        }
        OtlpProtocol::HttpProtobuf => {
            let mut exporter = MetricExporter::builder()
                .with_http()
                .with_endpoint(endpoint.to_string())
                .with_protocol(Protocol::HttpBinary);
            if !headers.is_empty() {
                exporter = exporter.with_headers(to_headers_map(headers));
            }
            exporter
                .build()
                .context("failed to build HTTP OTLP metric exporter")
        }
    }
}

pub(super) fn parse_otlp_headers(value: Option<String>) -> Vec<(String, String)> {
    value
        .as_deref()
        .map(|raw| {
            raw.split(',')
                .filter_map(|pair| {
                    let (key, value) = pair.split_once('=')?;
                    let key = key.trim();
                    let value = value.trim();
                    if key.is_empty() || value.is_empty() {
                        return None;
                    }
                    Some((key.to_string(), value.to_string()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn to_headers_map(headers: &[(String, String)]) -> HashMap<String, String> {
    headers.iter().cloned().collect()
}

fn to_metadata_map(
    headers: &[(String, String)],
) -> anyhow::Result<opentelemetry_otlp::tonic_types::metadata::MetadataMap> {
    let mut metadata = opentelemetry_otlp::tonic_types::metadata::MetadataMap::new();

    for (key, value) in headers {
        let metadata_key = key
            .parse::<MetadataKey<Ascii>>()
            .with_context(|| format!("invalid OTLP header key: {key}"))?;
        let metadata_value = value
            .parse::<MetadataValue<Ascii>>()
            .with_context(|| format!("invalid OTLP header value for key: {key}"))?;
        metadata.insert(metadata_key, metadata_value);
    }

    Ok(metadata)
}
