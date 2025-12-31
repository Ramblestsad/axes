use axes::route::route;
use axes::utils::gracefully_shutdown::shutdown_token;
use axes::utils::tracing_setup;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // log init
    tracing_setup::init_tracing_subscriber();

    // server build
    let http_addr: std::net::SocketAddr = "0.0.0.0:7878".parse()?;
    let grpc_addr: std::net::SocketAddr = "0.0.0.0:8787".parse()?;
    tracing::info!(
        "http listening on http://{} grpc listening on http://{}",
        http_addr,
        grpc_addr
    );

    let token = shutdown_token();

    let grpc_token = token.clone();
    let grpc_svc = axes::grpc::greeter_impl::router();
    let grpc_task = tokio::spawn(async move {
        let t = grpc_token.clone();
        Server::builder()
            .add_service(grpc_svc)
            .serve_with_shutdown(grpc_addr, async move {t.cancelled().await})
            .await?;
        anyhow::Ok(())
    });

    let app = route().await?;
    let http_task = tokio::spawn(async move {
        let t = token.clone();
        let listener = tokio::net::TcpListener::bind(http_addr).await?;
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {t.cancelled().await})
            .await?;
        anyhow::Ok(())
    });
    let (r1, r2) = tokio::join!(http_task, grpc_task);
    r1??;
    r2??;

    Ok(())
}
