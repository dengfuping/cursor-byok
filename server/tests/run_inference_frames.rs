//! Cursor's newer agent loop sends `aiserver.v1.InferenceService` frames on the
//! same Bidi append RPC. They are not `agent.v1.AgentRunRequest`, so they must be
//! reported as an unsupported protocol instead of being misread as a corrupt
//! conversation state (their `conversation_id` starts with bytes that look like
//! `ConversationStateStructure.file_states` / `.summary` / `.plan`).
use cursor_server::{
    api::cursor::bidi,
    cursor::protocol::proto::{agent::v1 as agent, aiserver::v1 as ai, inference::v1 as pb},
};
use prost::Message;

const CONVERSATION_ID: &str = "b18be67a-9c7c-41d8-841a-2244555cbb98";
const MODEL_ID: &str = "local-model";

fn run_request_frame() -> Vec<u8> {
    pb::RunInferenceClientMessage {
        message: Some(pb::run_inference_client_message::Message::RunRequest(
            pb::RunInferenceRunRequest {
                conversation_id: CONVERSATION_ID.into(),
                requested_model: Some(pb::InferenceRequestedModel {
                    model_id: MODEL_ID.into(),
                    ..Default::default()
                }),
                agent_mode: Some("agent".into()),
                turn_agent_mode: Some("agent".into()),
                routing_conversation: vec![pb::RunInferenceRoutingMessage {
                    text: "make the failing test pass".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )),
    }
    .encode_to_vec()
}

fn append(payload: Vec<u8>) -> ai::BidiAppendRequest {
    ai::BidiAppendRequest {
        request_id: Some(ai::BidiRequestId {
            request_id: "run-inference-frame".into(),
        }),
        append_seqno: 1,
        data_binary: payload,
        ..Default::default()
    }
}

/// The payload really is the frame that used to be misread as a state error.
#[test]
fn run_inference_frame_fails_the_agent_decoder() {
    let error = agent::AgentClientMessage::decode(run_request_frame().as_slice())
        .expect_err("the agent schema must not accept the inference frame");
    assert!(
        error.to_string().contains("conversation_state"),
        "unexpected agent decode error: {error}"
    );
}

#[test]
fn run_inference_frame_is_reported_as_unsupported_protocol() {
    let Err(error) = bidi::decode(&append(run_request_frame())) else {
        panic!("the frame must be rejected until the protocol is implemented");
    };
    let message = error.to_string();
    assert!(
        message.contains("RunInference"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("conversation_state"),
        "the failure must not blame the conversation state: {message}"
    );
}

#[test]
fn agent_frames_are_not_reclassified_as_inference() {
    // An `agent.v1` conversation-action append shares the `run_request` field
    // number, so the classifier must not swallow it.
    let frame = agent::AgentClientMessage {
        message: Some(agent::agent_client_message::Message::ConversationAction(
            agent::ConversationAction {
                action: Some(agent::conversation_action::Action::CancelAction(
                    agent::CancelAction::default(),
                )),
                ..Default::default()
            },
        )),
    };
    let decoded = bidi::decode(&append(frame.encode_to_vec()))
        .expect("an agent.v1 conversation action must keep decoding");
    assert!(matches!(
        decoded.message.message,
        Some(agent::agent_client_message::Message::ConversationAction(_))
    ));
}
