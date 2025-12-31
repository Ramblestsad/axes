pub mod greeter_impl;

pub mod greeter {
    tonic::include_proto!("greeter.v1"); // The string specified here must match the proto package name
}

pub fn router() -> tonic::transport::server::Router {
    tonic::transport::Server::builder().add_service(greeter_impl::router())
}
