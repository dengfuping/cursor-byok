//! Decodes the `aiserver.v1.InferenceService` frames that Cursor's newer agent
//! loop sends instead of `agent.v1.AgentRunRequest`.
//!
//! Cursor moved the agent run behind its own inference service: the client sends
//! `RunInferenceClientMessage` frames (`run_request`, then `invoke_model` /
//! `finish_run`) and the server drives the loop. A local BYOK server has to speak
//! this protocol to keep serving runs, so the frames are decoded here before the
//! run mapping is implemented.
use prost::Message;

use crate::{
    cursor::protocol::proto::{agent::v1 as agent, inference::v1 as pb},
    Error, Result,
};

/// A decoded `RunInferenceClientMessage`, keeping only the shape this server
/// cares about.
#[derive(Debug)]
pub enum Frame {
    RunRequest(Box<pb::RunInferenceRunRequest>),
    InvokeModel(Box<pb::RunInferenceInvokeModel>),
    FinishRun(pb::RunInferenceFinishRun),
    CancelInvocation(pb::RunInferenceCancelInvocation),
    Heartbeat,
    Other(&'static str),
}

/// Decodes one `RunInferenceClientMessage` payload, if it is one.
pub fn decode(payload: &[u8]) -> Option<Result<Frame>> {
    let message = pb::RunInferenceClientMessage::decode(payload).ok()?;
    Some(frame(message))
}

/// Recognises the frames this server can name, ignoring payloads that merely
/// happen to parse as some other message.
///
/// Both protocols start with a `run_request` field, so a plain decode is not
/// enough: an `agent.v1` append can parse as a `RunInferenceClientMessage` whose
/// string fields reject the bytes. Only a frame with the identifying fields of
/// the inference protocol counts.
pub fn classify(payload: &[u8]) -> Option<Frame> {
    match decode(payload)? {
        Ok(Frame::RunRequest(request)) => {
            let identified = !request.conversation_id.is_empty()
                && request.requested_model.is_some()
                && request
                    .routing_conversation
                    .iter()
                    .all(|m| !m.text.is_empty());
            identified.then_some(Frame::RunRequest(request))
        }
        Ok(Frame::InvokeModel(request)) => (!request.invocation_id.is_empty()
            && request.request.is_some())
        .then_some(Frame::InvokeModel(request)),
        Ok(Frame::CancelInvocation(request)) => {
            (!request.invocation_id.is_empty()).then_some(Frame::CancelInvocation(request))
        }
        // `finish_run`, heartbeats and the remaining frames share field numbers
        // with `agent.v1` messages, so they are only meaningful once a
        // RunInference run exists; they never start a classification.
        Ok(Frame::FinishRun(_)) | Ok(Frame::Heartbeat) | Ok(Frame::Other(_)) | Err(_) => None,
    }
}

fn frame(message: pb::RunInferenceClientMessage) -> Result<Frame> {
    use pb::run_inference_client_message::Message as Live;
    Ok(match message.message {
        Some(Live::RunRequest(request)) => Frame::RunRequest(Box::new(request)),
        Some(Live::InvokeModel(request)) => Frame::InvokeModel(Box::new(request)),
        Some(Live::FinishRun(request)) => Frame::FinishRun(request),
        Some(Live::CancelInvocation(request)) => Frame::CancelInvocation(request),
        Some(Live::ClientHeartbeat(_)) | Some(Live::ClientFirstCallTimings(_)) => Frame::Heartbeat,
        Some(Live::AgentEvent(_)) => Frame::Other("agent_event"),
        Some(Live::RunRequestLog(_)) => Frame::Other("run_request_log"),
        Some(Live::ResolveSubagentModels(_)) => Frame::Other("resolve_subagent_models"),
        Some(Live::DeltaBaseSeed(_)) => Frame::Other("delta_base_seed"),
        None => return Err(Error::Protocol("empty RunInference frame".into())),
    })
}

/// Model id the client asked for, matching how `agent.v1` requests are routed.
pub fn requested_model(request: &pb::RunInferenceRunRequest) -> Option<String> {
    request
        .requested_model
        .as_ref()
        .map(|model| model.model_id.clone())
        .filter(|model| !model.is_empty())
}

/// One-line description for the diagnostic log while the run mapping is missing.
pub fn describe(request: &pb::RunInferenceRunRequest) -> String {
    format!(
        "conversation_id={:?} model={:?} mode={:?} turn_mode={:?} prewarm={} routing_messages={}",
        request.conversation_id,
        requested_model(request),
        request.agent_mode,
        request.turn_agent_mode,
        request.prewarm,
        request.routing_conversation.len(),
    )
}

/// Conversion helper for the future run mapping: the model details the client
/// sent alongside the request, in the shape the existing run engine expects.
pub fn model_details(request: &pb::RunInferenceRunRequest) -> Option<agent::ModelDetails> {
    let requested = request.requested_model.as_ref()?;
    Some(agent::ModelDetails {
        model_id: requested.model_id.clone(),
        display_model_id: requested.model_id.clone(),
        ..Default::default()
    })
}

/// User-visible text the client attached for routing, newest last.
pub fn routing_text(request: &pb::RunInferenceRunRequest) -> Vec<&str> {
    request
        .routing_conversation
        .iter()
        .map(|message| message.text.as_str())
        .collect()
}
