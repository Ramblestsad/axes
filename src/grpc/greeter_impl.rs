use tonic::{Request, Response, Status};

use crate::grpc::greeter::greeter_server::{Greeter, GreeterServer};
use crate::grpc::greeter::{HelloReply, HelloRequest};

#[derive(Default)]
pub struct GreeterSvc;

#[tonic::async_trait]
impl Greeter for GreeterSvc {
    async fn say_hello(
        &self,
        req: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let name = req.into_inner().name;
        Ok(Response::new(HelloReply {
            message: format!("Hello {name}"),
        }))
    }
}

pub fn router() -> GreeterServer<GreeterSvc> {
    GreeterServer::new(GreeterSvc::default())
}
