use tracing_subscriber::{
    fmt, layer::SubscriberExt, util::SubscriberInitExt,
};

/// Initialize tracing subscriber.
pub fn init_tracing_subscriber() {
    let format = "[year]-[month padding:zero]-[day padding:zero] \
                         [hour]:[minute]:[second].[subsecond digits:4]";
    // let offset = time::UtcOffset::current_local_offset().unwrap_or_else(|_| time::UtcOffset::UTC);
    // in case current_local_offset() panics
    let offset = time::UtcOffset::from_hms(8, 0, 0).unwrap();
    let timer = time::format_description::parse(format).unwrap();
    let time_format = fmt::time::OffsetTime::new(offset, timer);

    tracing_subscriber::registry()
        // with() needs layer::SubscriberExt
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!(
                    "{}=debug,tower_http=debug,axum::rejection=trace",
                    env!("CARGO_CRATE_NAME")
                )
                .into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .compact()
                .with_ansi(true)
                // .with_file(true)
                .with_line_number(true)
                .with_timer(time_format.clone()),
        )
        // init() needs tracing_subscriber::util::SubscriberInitExt
        .init();

    // tracing::info!("Tracing initialized.");
}
