pub mod greeter_impl;

pub mod greeter {
    tonic::include_proto!("greeter.v1"); // The string specified here must match the proto package name
}

pub async fn serve(
    grpc_addr: std::net::SocketAddr,
    token: tokio_util::sync::CancellationToken,
) -> Result<(), tonic::transport::Error> {
    tonic::transport::Server::builder()
        .layer(crate::utils::observability::grpc_observability_layer())
        .add_service(greeter_impl::router())
        .serve_with_shutdown(grpc_addr, async move { token.cancelled().await })
        .await
}
