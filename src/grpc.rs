pub mod greeter_impl;

pub mod greeter {
    tonic::include_proto!("greeter.v1"); // The string specified here must match the proto package name
}
