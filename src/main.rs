use axes::route;
use axes::utils::gracefully_shutdown::shutdown_signal;
use axes::utils::tracing_setup::init_tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error>{
    // log init
    let _guard = init_tracing_subscriber();

    // server build
    let listener = tokio::net::TcpListener::bind("127.0.0.1:7878")
        .await
        .unwrap();
    tracing::info!(
        "listening on http://{}",
        listener
            .local_addr()
            .unwrap_or(std::net::SocketAddr::from(([127, 0, 0, 1], 7878)))
    );
    axum::serve(listener, route::route().await?)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server failed");

    Ok(())
}
