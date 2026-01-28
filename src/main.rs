use axes::{
    routes::route,
    utils::{
        gracefully_shutdown::shutdown_token,
        tracing_setup,
    },
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // log init
    tracing_setup::init_tracing_subscriber();

    // server build
    let http_addr: std::net::SocketAddr = std::env::var("AXES_HTTP_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:5173".to_string())
        .parse()?;
    let grpc_addr: std::net::SocketAddr = std::env::var("AXES_GRPC_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:5273".to_string())
        .parse()?;

    let token = shutdown_token();
    let router = route().await?;

    tracing::info!("http listening on http://{} grpc listening on http://{}", http_addr, grpc_addr);

    tokio::try_join!(
        run_http(http_addr, router, token.clone()),
        run_grpc(grpc_addr, token.clone()),
    )?;

    Ok(())
}

async fn run_http(
    http_addr: std::net::SocketAddr,
    router: axum::Router,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(http_addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(async move { token.cancelled().await })
        .await?;
    Ok(())
}

async fn run_grpc(
    grpc_addr: std::net::SocketAddr,
    token: tokio_util::sync::CancellationToken,
) -> anyhow::Result<()> {
    axes::grpc::router()
        .serve_with_shutdown(grpc_addr, async move { token.cancelled().await })
        .await?;
    Ok(())
}
