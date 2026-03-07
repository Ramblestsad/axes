use serde_json::{Map, Number, Value, json};
use tracing::{
    Event, Level, Subscriber,
    field::{Field, Visit},
};
use tracing_subscriber::{
    fmt::{
        self, FmtContext,
        format::{FormatEvent, FormatFields, Writer},
    },
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
};

use super::tracing::current_trace_context;

pub(super) fn init_tracing_subscriber(
    environment: &str,
    tracer: Option<opentelemetry_sdk::trace::Tracer>,
) {
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        format!("{}=debug,tower_http=debug,axum::rejection=trace", env!("CARGO_CRATE_NAME")).into()
    });

    if environment == "development" {
        init_development_tracing_subscriber(env_filter);
    } else {
        init_production_tracing_subscriber(env_filter, environment.to_string(), tracer);
    }
}

fn init_development_tracing_subscriber(env_filter: tracing_subscriber::EnvFilter) {
    let format = "[year]-[month padding:zero]-[day padding:zero] \
                         [hour]:[minute]:[second].[subsecond digits:4]";
    let offset = time::UtcOffset::from_hms(8, 0, 0).unwrap();
    let timer = time::format_description::parse(format).unwrap();
    let time_format = fmt::time::OffsetTime::new(offset, timer);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .compact()
                .with_ansi(true)
                .with_line_number(true)
                .with_timer(time_format),
        )
        .init();
}

fn init_production_tracing_subscriber(
    env_filter: tracing_subscriber::EnvFilter,
    environment: String,
    tracer: Option<opentelemetry_sdk::trace::Tracer>,
) {
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stdout)
        .event_format(OtelJsonFormatter::new(environment));

    match tracer {
        Some(tracer) => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(tracing_opentelemetry::layer().with_tracer(tracer))
            .init(),
        None => tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .init(),
    }
}

struct OtelJsonFormatter {
    deployment_environment: String,
}

impl OtelJsonFormatter {
    fn new(deployment_environment: String) -> Self {
        Self { deployment_environment }
    }
}

impl<S, N> FormatEvent<S, N> for OtelJsonFormatter
where
    S: Subscriber + for<'span> LookupSpan<'span>,
    N: for<'writer> FormatFields<'writer> + 'static,
{
    fn format_event(
        &self,
        _ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> std::fmt::Result {
        let mut visitor = JsonVisitor::default();
        event.record(&mut visitor);

        let metadata = event.metadata();
        visitor.insert_attribute("target", Value::String(metadata.target().to_string()));

        if let Some(module_path) = metadata.module_path() {
            visitor.insert_attribute("module_path", Value::String(module_path.to_string()));
        }

        if let Some(file) = metadata.file() {
            visitor.insert_attribute("file", Value::String(file.to_string()));
        }

        if let Some(line) = metadata.line() {
            visitor.insert_attribute("line", Value::Number(Number::from(line)));
        }

        let (severity_number, severity_text) = severity(metadata.level());
        let event_timestamp = unix_time_nanos();
        let observed_timestamp = unix_time_nanos();
        let body = visitor.body.unwrap_or_else(|| metadata.name().to_string());
        let (trace_id, span_id) =
            current_trace_context().unwrap_or_else(|| (String::new(), String::new()));

        let payload = json!({
            "time_unix_nano": event_timestamp,
            "observed_time_unix_nano": observed_timestamp,
            "severity_number": severity_number,
            "severity_text": severity_text,
            "body": body,
            "trace_id": trace_id,
            "span_id": span_id,
            "resource": {
                "service.name": env!("CARGO_PKG_NAME"),
                "service.version": env!("CARGO_PKG_VERSION"),
                "deployment.environment": self.deployment_environment,
            },
            "attributes": visitor.attributes,
        });

        let rendered = serde_json::to_string(&payload).map_err(|_| std::fmt::Error)?;
        writer.write_str(&rendered)?;
        writer.write_char('\n')
    }
}

#[derive(Default)]
struct JsonVisitor {
    body: Option<String>,
    attributes: Map<String, Value>,
}

impl JsonVisitor {
    fn insert_attribute(&mut self, key: impl Into<String>, value: Value) {
        self.attributes.insert(key.into(), value);
    }

    fn record_json_value(&mut self, field: &Field, value: Value) {
        if field.name() == "message" {
            self.body = Some(match value {
                Value::String(text) => text,
                other => other.to_string(),
            });
            return;
        }

        self.insert_attribute(field.name(), value);
    }
}

impl Visit for JsonVisitor {
    fn record_f64(&mut self, field: &Field, value: f64) {
        match Number::from_f64(value) {
            Some(number) => self.record_json_value(field, Value::Number(number)),
            None => self.record_json_value(field, Value::String(value.to_string())),
        }
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        self.record_json_value(field, Value::Number(Number::from(value)));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.record_json_value(field, Value::Number(Number::from(value)));
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        match Number::from_i128(value) {
            Some(number) => self.record_json_value(field, Value::Number(number)),
            None => self.record_json_value(field, Value::String(value.to_string())),
        }
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        match Number::from_u128(value) {
            Some(number) => self.record_json_value(field, Value::Number(number)),
            None => self.record_json_value(field, Value::String(value.to_string())),
        }
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.record_json_value(field, Value::Bool(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_json_value(field, Value::String(value.to_string()));
    }

    fn record_bytes(&mut self, field: &Field, value: &[u8]) {
        self.record_json_value(field, Value::String(format!("{value:?}")));
    }

    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.record_json_value(field, Value::String(value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.record_json_value(field, Value::String(format!("{value:?}")));
    }
}

fn severity(level: &Level) -> (u8, &'static str) {
    match *level {
        Level::TRACE => (1, "TRACE"),
        Level::DEBUG => (5, "DEBUG"),
        Level::INFO => (9, "INFO"),
        Level::WARN => (13, "WARN"),
        Level::ERROR => (17, "ERROR"),
    }
}

fn unix_time_nanos() -> String {
    let now = time::OffsetDateTime::now_utc();
    let seconds = i128::from(now.unix_timestamp());
    let nanos = i128::from(now.nanosecond());
    (seconds * 1_000_000_000 + nanos).to_string()
}
