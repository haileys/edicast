use std::net::SocketAddr;
use std::sync::Arc;
use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures::Future;
use http_body_util::BodyExt;
use http_body_util::combinators::BoxBody;
use hyper::body::{self, Body, Frame};
use hyper::server::conn::http1;
use hyper::{Request, Response, StatusCode};
use slog::Logger;
use thiserror::Error;
use uuid::Uuid;

use crate::audio::encode;
use crate::net;
use crate::stream::StreamSubscription;
use super::common;
use super::Edicast;

pub async fn start(address: SocketAddr, edicast: Arc<Edicast>)
    -> Result<impl Future<Output = ()>, net::BindError>
{
    let listener = net::bind(address).await?;

    let _ = crate::thread::spawn_worker("edicast/public", async move {
        loop {
            let log = slog_scope::logger().new(slog::o!("service" => "public"));

            let (stream, peer) = match listener.accept().await {
                Ok(result) => result,
                Err(err) => {
                    slog::warn!(log, "error accepting connection: {}", err);
                    continue;
                }
            };

            let service = hyper::service::service_fn({
                let log = log.clone();
                let edicast = edicast.clone();
                move |mut req| {
                    req.extensions_mut().insert(net::SocketPeer(peer));
                    dispatch(req, log.clone(), edicast.clone())
                }
            });

            tokio::task::spawn_local(async move {
                let result = http1::Builder::new()
                    .serve_connection(stream, service)
                    .await;

                match result {
                    Ok(()) => {}
                    Err(err) => {
                        slog::warn!(log, "error serving connection: {}", err);
                    }
                }
            });
        }
    });

    // accept loop in worker thread never terminates
    Ok(futures::future::pending::<()>())
}

type DispatchResponse = Response<BoxBody<Bytes, ClientLagged>>;

fn not_found() -> DispatchResponse {
    common::status(StatusCode::NOT_FOUND)
        .map(|body| body.map_err(|_| -> ClientLagged { unreachable!() }).boxed())
}

async fn dispatch(req: Request<body::Incoming>, log: Logger, edicast: Arc<Edicast>)
    -> Result<DispatchResponse, ClientLagged>
{
    let request_id = Uuid::new_v4();
    let log = log.new(slog::o!("request_id" => request_id));

    let path = req.uri().path();

    let stream_id = match edicast.public_routes.get(path) {
        Some(stream_id) => stream_id,
        None => { return Ok(not_found()); }
    };

    let content_type = encode::mime_type_from_config(
        &edicast.config.stream[stream_id].codec);

    let stream = match edicast.streams.subscribe_stream(stream_id) {
        Some(stream) => stream,
        None => { return Ok(not_found()); }
    };

    slog::info!(log, "Listener connected";
        "stream" => stream_id,
        common::request_log_keys_hyper(&req),
    );

    let response = Response::builder()
        .header("content-type", content_type)
        .header("cache-control", "no-store")
        .status(StatusCode::OK)
        .body(StreamBody(stream).boxed())
        .expect("build response");

    Ok(response)
}

#[derive(Error, Debug)]
#[error("client lagged too far behind stream")]
pub struct ClientLagged;

struct StreamBody(StreamSubscription);

impl Body for StreamBody {
    type Data = Bytes;
    type Error = ClientLagged;

    fn poll_frame(mut self: Pin<&mut Self>, cx: &mut Context<'_>)
        -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>>
    {
        use tokio::sync::broadcast::error::RecvError;

        // recv is cancel-safe, so it's safe to call it again on every poll
        let mut self_ = self.as_mut();
        let recv = self_.0.recv();
        futures::pin_mut!(recv);

        recv.poll(cx).map(|result| {
            match result {
                Ok(bytes) => Some(Ok(Frame::data(bytes))),
                Err(RecvError::Closed) => None,
                Err(RecvError::Lagged(_)) => Some(Err(ClientLagged)),
            }
        })
    }
}
