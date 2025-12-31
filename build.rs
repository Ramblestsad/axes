fn main() -> Result<(), Box<dyn std::error::Error>> {
    build_grpc_protos()?;

    Ok(())
}

fn build_grpc_protos() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=protos");

    let protoc_path = protoc_bin_vendored::protoc_bin_path()?;
    unsafe { std::env::set_var("PROTOC", protoc_path) };

    tonic_prost_build::compile_protos("./protos/greeter.proto")?;

    Ok(())
}
