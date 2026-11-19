//! Streams `aiserver.v1.InferenceService` frames to the Cursor agent host.
//!
//! The agent host sends `RunInferenceClientMessage` frames on
//! `BidiService/BidiAppend` and reads `RunInferenceServerMessage` frames from
//! `InferenceService/RunInferenceSSE`, both keyed by the same request id.
use std::{collections::HashMap, sync::Arc};

use bytes::Bytes;
use tokio::sync::{mpsc, Mutex};

use crate::{
    cursor::protocol::{connect, proto::inference::v1 as pb},
    Result,
};

/// Frames that arrive before the agent host opens its stream are replayed, so
/// `run_ready` is never lost to the subscribe/open race.
const MAX_QUEUED_FRAMES: usize = 256;

#[derive(Default)]
struct Channel {
    sender: Option<mpsc::UnboundedSender<Bytes>>,
    queued: Vec<Bytes>,
    conversation_id: Option<String>,
    resolved_model: Option<String>,
}

/// Routes `RunInferenceServerMessage` frames to the SSE stream of one request.
#[derive(Clone, Default)]
pub struct InferenceHub {
    inner: Arc<Mutex<HashMap<String, Channel>>>,
}

impl InferenceHub {
    /// Attaches a new output stream to `request_id`, replacing any previous one.
    ///
    /// Returns the frames already queued for this request, in order.
    pub async fn subscribe(
        &self,
        request_id: &str,
    ) -> (Vec<Bytes>, mpsc::UnboundedReceiver<Bytes>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let mut inner = self.inner.lock().await;
        let channel = inner.entry(request_id.to_owned()).or_default();
        channel.sender = Some(sender);
        (std::mem::take(&mut channel.queued), receiver)
    }

    /// Sends one frame, queueing it while the agent host has not subscribed yet.
    pub async fn publish(
        &self,
        request_id: &str,
        message: &pb::RunInferenceServerMessage,
    ) -> Result<()> {
        let frame = connect::encode_message(message)?;
        let mut inner = self.inner.lock().await;
        let channel = inner.entry(request_id.to_owned()).or_default();
        match &channel.sender {
            Some(sender) => {
                let _ = sender.send(frame);
            }
            None if channel.queued.len() < MAX_QUEUED_FRAMES => channel.queued.push(frame),
            None => {}
        }
        Ok(())
    }

    pub async fn detach(&self, request_id: &str) {
        self.inner.lock().await.remove(request_id);
    }

    /// Remembers which conversation the request belongs to, for provider calls.
    pub async fn set_conversation(&self, request_id: &str, conversation_id: &str) {
        let mut inner = self.inner.lock().await;
        inner
            .entry(request_id.to_owned())
            .or_default()
            .conversation_id = Some(conversation_id.to_owned());
    }

    /// Remembers the model named in `run_ready`; `invoke_model` does not carry it.
    pub async fn set_resolved_model(&self, request_id: &str, model_id: &str) {
        let mut inner = self.inner.lock().await;
        inner
            .entry(request_id.to_owned())
            .or_default()
            .resolved_model = Some(model_id.to_owned());
    }

    pub async fn resolved_model(&self, request_id: &str) -> Option<String> {
        self.inner
            .lock()
            .await
            .get(request_id)
            .and_then(|channel| channel.resolved_model.clone())
    }

    pub async fn conversation(&self, request_id: &str) -> Option<String> {
        self.inner
            .lock()
            .await
            .get(request_id)
            .and_then(|channel| channel.conversation_id.clone())
    }

    /// Whether this request already runs on the inference protocol.
    ///
    /// `finish_run` and the remaining frames carry no identifying field, so the
    /// established run is what tells them apart from `agent.v1` appends.
    pub async fn is_active(&self, request_id: &str) -> bool {
        self.inner.lock().await.contains_key(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost::Message;

    fn frame(text: &str) -> pb::RunInferenceServerMessage {
        use pb::run_inference_server_message::Message as Server;

        pb::RunInferenceServerMessage {
            message: Some(Server::InvocationResponse(
                pb::RunInferenceInvocationResponse {
                    invocation_id: "invocation".into(),
                    response: Some(pb::InferenceStreamResponse {
                        response: Some(pb::inference_stream_response::Response::TextPart(
                            pb::InferenceTextStreamPart {
                                text: text.into(),
                                is_final: true,
                            },
                        )),
                        ..Default::default()
                    }),
                },
            )),
        }
    }

    /// `run_ready` is published before the agent host opens its stream.
    #[tokio::test]
    async fn frames_published_before_subscribe_are_replayed() {
        let hub = InferenceHub::default();
        hub.publish("request", &frame("ready")).await.unwrap();

        let (queued, mut receiver) = hub.subscribe("request").await;
        assert_eq!(queued.len(), 1);
        assert!(receiver.try_recv().is_err(), "queued frames are sent once");

        hub.publish("request", &frame("after")).await.unwrap();
        let live = receiver
            .try_recv()
            .expect("frame published after subscribing");
        assert_eq!(live.len(), queued[0].len(), "same framing on both paths");
    }

    #[tokio::test]
    async fn queued_frames_keep_their_order() {
        let hub = InferenceHub::default();
        hub.publish("request", &frame("first")).await.unwrap();
        hub.publish("request", &frame("second")).await.unwrap();

        let (queued, _receiver) = hub.subscribe("request").await;
        assert_eq!(queued.len(), 2);
        let decoded: Vec<_> = queued
            .iter()
            .map(|bytes| pb::RunInferenceServerMessage::decode(&bytes[5..]).unwrap())
            .collect();
        assert!(matches!(
            decoded[0].message,
            Some(pb::run_inference_server_message::Message::InvocationResponse(_))
        ));
    }
}
