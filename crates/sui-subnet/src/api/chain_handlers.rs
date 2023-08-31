use std::io;
use std::marker::PhantomData;

use avalanche_types::proto::http::Element;
use avalanche_types::subnet::rpc::http::handle::Handle;
use bytes::Bytes;
use jsonrpc_core::{BoxFuture, Error, ErrorCode, IoHandler, Result};
use jsonrpc_derive::rpc;
use serde::{Deserialize, Serialize};

use crate::api::de_request;
use crate::vm::Vm;

#[rpc]
pub trait Rpc {

}



#[derive(Clone)]
pub struct ChainService {
    pub vm: Vm,
}

impl ChainService {
    pub fn new(vm: Vm) -> Self {
        Self { vm }
    }
}


impl Rpc for ChainService {

}

#[derive(Clone, Debug)]
pub struct ChainHandler<T> {
    pub handler: IoHandler,
    _marker: PhantomData<T>,
}

#[tonic::async_trait]
impl<T> Handle for ChainHandler<T>
    where
        T: Rpc + Send + Sync + Clone + 'static,
{
    async fn request(
        &self,
        req: &Bytes,
        _headers: &[Element],
    ) -> io::Result<(Bytes, Vec<Element>)> {
        match self.handler.handle_request(&de_request(req)?).await {
            Some(resp) => Ok((Bytes::from(resp), Vec::new())),
            None => Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to handle request",
            )),
        }
    }
}

impl<T: Rpc> ChainHandler<T> {
    pub fn new(service: T) -> Self {
        let mut handler = jsonrpc_core::IoHandler::new();
        handler.extend_with(Rpc::to_delegate(service));
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}


fn create_jsonrpc_error(e: std::io::Error) -> Error {
    let mut error = Error::new(ErrorCode::InternalError);
    error.message = format!("{}", e);
    error
}
