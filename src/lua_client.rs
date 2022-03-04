use mlua::prelude::*;

use axum::{
    body::Body,
    http::{Error as HttpError, Method, Request, Uri},
};
use serde_json;
use std::io;
use std::pin::Pin;
use std::string::FromUtf8Error;
use std::task::{Context, Poll};
use tokio::runtime::Runtime; // 0.3.5

use super::server;
use hyper::client::connect::{Connected, Connection};

use std::path::PathBuf;
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::UnixStream,
};

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),

    #[error("{0}")]
    HyperError(#[from] hyper::Error),

    #[error("{0}")]
    HttpError(#[from] HttpError),

    #[error("{0}")]
    JsonError(#[from] serde_json::Error),

    #[error("{0}")]
    FromUtf8Error(#[from] FromUtf8Error),
}
// TODO(tacogips) try to use LuaTcpStream
// https://github.com/khvzak/mlua/blob/master/examples/async_tcp_server.rs
fn run_cmd(_lua: &Lua, (cmd_name, input): (String, String)) -> LuaResult<String> {
    let result = Runtime::new().unwrap().block_on(build_client_and_request(
        &cmd_name,
        server::default_socket_path(),
        input,
    ));
    match result {
        Ok(result) => Ok(result.output),
        Err(e) => Ok(format!("error:{}", e)),
    }
}

async fn build_client_and_request(
    cmd_name: &str,
    socket_path: &'static PathBuf,
    input: String,
) -> Result<server::RunCmdResponse, ClientError> {
    let connector = tower::service_fn(move |_: Uri| {
        let path = socket_path.clone();
        Box::pin(async move {
            let stream = UnixStream::connect(path).await?;
            Ok::<_, io::Error>(ClientConnection { stream })
        })
    });
    let client = hyper::Client::builder().build(connector);

    let req_body = server::RunCmdRequest {
        input,
        output_size: None,
    };
    let req_body_bytes = serde_json::to_vec(&req_body)?;

    let request = Request::builder()
        .method(Method::POST)
        .header("Content-Type", "application/json")
        .uri(format!("http://localhost/cmd/{}", cmd_name))
        .body(Body::from(req_body_bytes))?;

    let response = client.request(request).await?;

    let body = hyper::body::to_bytes(response.into_body()).await?;
    let resp: server::RunCmdResponse = serde_json::from_slice(&body)?;
    Ok(resp)
}

struct ClientConnection {
    stream: UnixStream,
}

impl AsyncRead for ClientConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(cx, buf)
    }
}

impl AsyncWrite for ClientConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Pin::new(&mut self.stream).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.stream).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.stream).poll_shutdown(cx)
    }
}

impl Connection for ClientConnection {
    fn connected(&self) -> Connected {
        Connected::new()
    }
}

#[mlua::lua_module]
fn dairi(lua: &Lua) -> LuaResult<LuaTable> {
    let exports = lua.create_table()?;
    //TODO(tacogips) create_async_function seems not compatible with tokio 1.17
    exports.set("run_cmd", lua.create_function(run_cmd)?)?;
    //exports.set("greet_people", lua.create_function(hello)?)?;
    Ok(exports)
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::server;
    #[ignore]
    #[tokio::test]
    async fn test_req() {
        let result =
            build_client_and_request("julia", server::default_socket_path(), "1+1\n".to_string())
                .await
                .unwrap();
        assert_eq!(
            server::RunCmdResponse {
                output: "2\n".to_string()
            },
            result
        )
    }
}
