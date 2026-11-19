//! Subscribes the Cursor agent host to `aiserver.v1.InferenceService` output.
//!
//! The agent host opens `RunInferenceSSE` with the request id it used for the
//! `BidiAppend` frames and reads `RunInferenceServerMessage` frames from it.
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderValue, Request, Response, StatusCode},
};
use bytes::Bytes;
use std::convert::Infallible;
use tokio::sync::mpsc;

use crate::{
    cursor::{
        protocol::{connect, proto::aiserver::v1 as ai},
        transport::{InferenceHub, TransportRegistry},
    },
    Result,
};

/// Stops the provider calls of a request once the agent host closes the stream.
struct StreamGuard {
    hub: InferenceHub,
    request_id: String,
}

impl Drop for StreamGuard {
    fn drop(&mut self) {
        let hub = self.hub.clone();
        let request_id = self.request_id.clone();
        tokio::spawn(async move { hub.cancel_request(&request_id).await });
    }
}

pub async fn stream(
    State(registry): State<TransportRegistry>,
    request: Request<Body>,
) -> Result<Response<Body>> {
    let body = to_bytes(request.into_body(), usize::MAX)
        .await
        .map_err(|error| crate::Error::Protocol(format!("cannot read request body: {error}")))?;
    let request: ai::BidiRequestId = connect::decode_unary(&body)?;
    let hub = registry.inference();
    let (queued, receiver) = hub.subscribe(&request.request_id).await;
    tracing::info!(
        request_id = request.request_id,
        queued = queued.len(),
        "agent host subscribed to RunInferenceSSE"
    );
    Ok(frame_stream(hub, request.request_id, queued, receiver))
}

fn frame_stream(
    hub: InferenceHub,
    request_id: String,
    queued: Vec<Bytes>,
    mut receiver: mpsc::UnboundedReceiver<Bytes>,
) -> Response<Body> {
    let stream = async_stream::stream! {
        let _guard = StreamGuard { hub, request_id };
        for frame in queued {
            yield Ok::<Bytes, Infallible>(frame);
        }
        while let Some(chunk) = receiver.recv().await {
            yield Ok::<Bytes, Infallible>(chunk);
        }
        yield Ok(connect::encode_end_stream());
    };
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
        .headers_mut()
        .insert("connect-protocol-version", HeaderValue::from_static("1"));
    response
}
