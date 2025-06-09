use axes::route::route;
use axes::utils::gracefully_shutdown::shutdown_signal;
use axes::utils::tracing_setup;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // log init
    tracing_setup::init_tracing_subscriber();

    // server build
    let listener = tokio::net::TcpListener::bind("0.0.0.0:7878").await.unwrap();
    tracing::info!(
        "listening on http://{}",
        listener
            .local_addr()
            .unwrap_or(std::net::SocketAddr::from(([127, 0, 0, 1], 7878)))
    );
    let app = route().await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server failed");

    Ok(())
}
