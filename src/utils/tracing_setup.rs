use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize tracing subscriber.
pub fn init_tracing_subscriber() -> WorkerGuard {
    let format = "[year]-[month padding:zero]-[day padding:zero] \
                         [hour]:[minute]:[second].[subsecond digits:4]";
    // let offset = time::UtcOffset::current_local_offset().unwrap_or_else(|_| time::UtcOffset::UTC);
    // in case current_local_offset() panics
    let offset = time::UtcOffset::from_hms(8, 0, 0).unwrap();
    let timer = time::format_description::parse(format).unwrap();
    let time_format = fmt::time::OffsetTime::new(offset, timer);

    let rolling_log = rolling::never("./logs/", "run.log");
    let (non_blocking_apd, guard) = tracing_appender::non_blocking(rolling_log);

    let file_layer = fmt::layer()
        .with_writer(non_blocking_apd)
        .with_ansi(false)
        .with_line_number(true)
        .with_timer(time_format.clone())
        .compact();
    let fmt_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_line_number(true)
        .with_timer(time_format.clone());

    tracing_subscriber::registry()
        // with() needs `tracing_subscriber::layer::SubscriberExt`
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "axes=debug,tower_http=debug,axum::rejection=trace".into()
            }),
        )
        .with(file_layer)
        .with(fmt_layer)
        // init() needs `tracing_subscriber::util::SubscriberInitExt`
        .init();
    tracing::info!("Tracing initialized.");

    guard
}
