use crate::process_manager;
use serde::{Deserialize, Serialize};

use axum::{
    error_handling::HandleErrorLayer,
    extract::connect_info,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use futures::ready;
use std::string::FromUtf8Error;
use std::time::Duration;
use thiserror::Error;
use tokio::net::{unix::UCred, UnixListener, UnixStream};
use tower::ServiceBuilder;

use hyper::server::accept::Accept;
use once_cell::sync::OnceCell;
use std::{
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tower::BoxError;

pub static DEFAULT_SOCKET_PATH: OnceCell<PathBuf> = OnceCell::new();
pub fn default_socket_path() -> &'static PathBuf {
    DEFAULT_SOCKET_PATH.get_or_init(|| PathBuf::from("/tmp/dairi/serve.sock"))
}

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("{0}")]
    IoError(#[from] std::io::Error),
}
const REQUEST_TIMEOUT_SEC: u64 = 180;
pub async fn serve() -> Result<(), ServerError> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug")
    }

    let socket_path = default_socket_path();
    let _ = tokio::fs::remove_file(&socket_path).await;
    tokio::fs::create_dir_all(socket_path.parent().unwrap()).await?;
    let uds = UnixListener::bind(socket_path.clone()).unwrap();

    let app = Router::new().route("/cmd/:cmd_name", post(run_cmd)).layer(
        ServiceBuilder::new()
            .layer(HandleErrorLayer::new(|error: BoxError| async move {
                if error.is::<tower::timeout::error::Elapsed>() {
                    Ok(StatusCode::REQUEST_TIMEOUT)
                } else {
                    Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Unhandled internal error: {}", error),
                    ))
                }
            }))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SEC))
            .into_inner(),
    );

    tracing::info!("dairi server is listening at {}", socket_path.display());

    axum::Server::builder(ServerAccept { uds })
        .serve(app.into_make_service_with_connect_info::<UdsConnectInfo, _>())
        .await
        .unwrap();

    Ok(())
}

struct ServerAccept {
    uds: UnixListener,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
struct UdsConnectInfo {
    peer_addr: Arc<tokio::net::unix::SocketAddr>,
    peer_cred: UCred,
}

impl connect_info::Connected<&UnixStream> for UdsConnectInfo {
    fn connect_info(target: &UnixStream) -> Self {
        let peer_addr = target.peer_addr().unwrap();
        let peer_cred = target.peer_cred().unwrap();

        Self {
            peer_addr: Arc::new(peer_addr),
            peer_cred,
        }
    }
}

impl Accept for ServerAccept {
    type Conn = UnixStream;
    type Error = BoxError;

    fn poll_accept(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Self::Conn, Self::Error>>> {
        let (stream, _addr) = ready!(self.uds.poll_accept(cx))?;
        Poll::Ready(Some(Ok(stream)))
    }
}

#[derive(Serialize, Deserialize)]
pub struct RunCmdRequest {
    pub input: String,
    pub output_size: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct RunCmdResponse {
    pub output: String,
}

async fn run_cmd(
    Path(cmd_name): Path<process_manager::CmdName>,
    Json(payload): Json<RunCmdRequest>,
) -> Result<Json<RunCmdResponse>, RunCmdError> {
    tracing::debug!("run cmd start {}", cmd_name);
    let output = process_manager::run_cmd(&cmd_name, payload.input, payload.output_size).await?;

    tracing::debug!("cmd finished [{}]", String::from_utf8(output.clone())?);
    let output = String::from_utf8(output)?;

    tracing::info!("cmd:{}, output:  {}", cmd_name, output);
    Ok(Json(RunCmdResponse { output }))
}

#[derive(Debug, Error)]
pub enum RunCmdError {
    #[error("{0}")]
    ProcessManagerError(#[from] process_manager::ProcessManagerError),

    #[error("{0}")]
    FromUtf8Error(#[from] FromUtf8Error),
}

impl IntoResponse for RunCmdError {
    fn into_response(self) -> Response {
        let status_code = StatusCode::BAD_REQUEST;
        let body = Json(RunCmdResponse {
            output: format!("{}", self),
        });

        (status_code, body).into_response()
    }
}
