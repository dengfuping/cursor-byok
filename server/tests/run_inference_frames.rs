//! Cursor's newer agent loop sends `aiserver.v1.InferenceService` frames on the
//! same Bidi append RPC. They are not `agent.v1.AgentRunRequest`, so they must be
//! reported as an unsupported protocol instead of being misread as a corrupt
//! conversation state (their `conversation_id` starts with bytes that look like
//! `ConversationStateStructure.file_states` / `.summary` / `.plan`).
use cursor_server::{
    api::cursor::bidi,
    cursor::protocol::{
        inference,
        proto::{agent::v1 as agent, aiserver::v1 as ai, inference::v1 as pb},
    },
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
fn run_inference_frame_is_decoded_as_a_run_request() {
    let decoded = bidi::decode(&append(run_request_frame()), false)
        .expect("the frame must decode instead of failing as a corrupt state");
    match decoded {
        bidi::Decoded::Inference { request_id, frame } => {
            assert_eq!(request_id, "run-inference-frame");
            match frame {
                inference::Frame::RunRequest(request) => {
                    assert_eq!(request.conversation_id, CONVERSATION_ID);
                    assert_eq!(
                        inference::requested_model(&request).as_deref(),
                        Some(MODEL_ID)
                    );
                }
                other => panic!("expected a run_request frame, got {other:?}"),
            }
        }
        _ => panic!("expected an inference frame"),
    }
}

#[test]
fn finish_run_is_accepted_once_the_run_uses_inference() {
    let finish = pb::RunInferenceClientMessage {
        message: Some(pb::run_inference_client_message::Message::FinishRun(
            pb::RunInferenceFinishRun {
                turn_completed: Some(true),
                ..Default::default()
            },
        )),
    }
    .encode_to_vec();

    // Without an established run it stays undecodable, so an agent.v1 append is
    // never mistaken for it.
    assert!(bidi::decode(&append(finish.clone()), false).is_err());

    match bidi::decode(&append(finish), true).expect("an active run accepts finish_run") {
        bidi::Decoded::Inference { frame, .. } => {
            assert!(matches!(frame, inference::Frame::FinishRun(_)));
        }
        _ => panic!("expected an inference frame"),
    }
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
    let decoded = bidi::decode(&append(frame.encode_to_vec()), false)
        .expect("an agent.v1 conversation action must keep decoding")
        .into_agent();
    assert!(matches!(
        decoded.message.message,
        Some(agent::agent_client_message::Message::ConversationAction(_))
    ));
}
