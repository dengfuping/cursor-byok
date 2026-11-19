//! Regression coverage for Bidi appends whose Cursor conversation state is corrupt.
//!
//! A Cursor client can hand us a `ConversationStateStructure` whose nested length
//! prefix does not match its payload, for example a `file_states` entry written
//! with a stale length after the value was trimmed. The state is opaque to this
//! server, so one bad entry must not fail the whole append: the run still needs
//! the action, the model, and every other checkpoint field.
use cursor_server::{
    api::cursor::bidi,
    cursor::protocol::{
        proto::{agent::v1 as pb, aiserver::v1 as ai},
        salvage,
    },
};
use prost::{encoding::WireType, Message};

const KEEP_PATH: &str = "src/keep.ts";
const DROP_PATH: &str = "src/drop.ts";
const CONVERSATION_ID: &str = "conversation-with-corrupt-state";
const MODEL_ID: &str = "local-model";

/// Encodes one length-delimited field, matching prost's wire format.
fn push_len_delimited(buf: &mut Vec<u8>, tag: u32, payload: &[u8]) {
    prost::encoding::encode_key(tag, WireType::LengthDelimited, buf);
    prost::encoding::encode_varint(payload.len() as u64, buf);
    buf.extend_from_slice(payload);
}

/// Wraps a `ConversationStateStructure` payload in a run request and its client
/// envelope, exactly as the Cursor client does.
fn append_payload(state: &[u8]) -> Vec<u8> {
    let run_request = pb::AgentRunRequest {
        conversation_id: Some(CONVERSATION_ID.into()),
        requested_model: Some(pb::RequestedModel {
            model_id: MODEL_ID.into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut request_bytes = run_request.encode_to_vec();
    // Field 1 of AgentRunRequest is conversation_state.
    push_len_delimited(&mut request_bytes, 1, state);

    let mut payload = Vec::new();
    // Field 1 of AgentClientMessage is the run request oneof.
    push_len_delimited(&mut payload, 1, &request_bytes);
    payload
}

fn state_turns() -> Vec<Vec<u8>> {
    vec![b"turn-blob-id".to_vec()]
}

fn valid_append_payload() -> Vec<u8> {
    let state = pb::ConversationStateStructure {
        turns: state_turns(),
        summary: Some(b"summary-bytes".to_vec()),
        file_states: [(KEEP_PATH.to_string(), b"keep".to_vec())]
            .into_iter()
            .collect(),
        ..Default::default()
    }
    .encode_to_vec();
    append_payload(&state)
}

/// Appends a `file_states` map entry whose value outlives the entry's own length.
///
/// The declared entry length is one byte shorter than the bytes written, so a
/// decoder reads the value past the entry boundary and reports
/// "delimited length exceeded" for `ConversationStateStructure.file_states`.
fn append_corrupt_file_state(state: &mut Vec<u8>, path: &str, value: &[u8]) {
    let mut entry = Vec::new();
    push_len_delimited(&mut entry, 1, path.as_bytes());
    push_len_delimited(&mut entry, 2, value);
    // Field 12 of ConversationStateStructure is the legacy file_states map.
    prost::encoding::encode_key(12, WireType::LengthDelimited, state);
    prost::encoding::encode_varint((entry.len() - 1) as u64, state);
    state.extend_from_slice(&entry);
}

fn corrupt_append_payload() -> Vec<u8> {
    let mut state = pb::ConversationStateStructure {
        turns: state_turns(),
        summary: Some(b"summary-bytes".to_vec()),
        file_states: [(KEEP_PATH.to_string(), b"keep".to_vec())]
            .into_iter()
            .collect(),
        ..Default::default()
    }
    .encode_to_vec();
    append_corrupt_file_state(&mut state, DROP_PATH, b"a value the client trimmed");
    append_payload(&state)
}

/// The same corruption in the middle of the state: everything encoded after the
/// bad entry is unreadable and must be dropped rather than fail the append.
fn mid_state_corrupt_payload() -> Vec<u8> {
    let mut state = Vec::new();
    push_len_delimited(&mut state, 8, b"turn-blob-id");
    append_corrupt_file_state(&mut state, DROP_PATH, b"a value the client trimmed");
    push_len_delimited(&mut state, 6, b"summary-bytes");
    append_payload(&state)
}

/// The shape the app hits in the wild: the client writes a
/// `conversation_state` length that runs past the state and swallows the fields
/// that follow it, so `action` ends up inside the state region.
fn overlong_state_payload(extra: usize) -> Vec<u8> {
    let state = pb::ConversationStateStructure {
        turns: state_turns(),
        summary: Some(b"summary-bytes".to_vec()),
        file_states: [(KEEP_PATH.to_string(), b"keep".to_vec())]
            .into_iter()
            .collect(),
        ..Default::default()
    }
    .encode_to_vec();
    let action = pb::ConversationAction {
        action: Some(pb::conversation_action::Action::UserMessageAction(
            pb::UserMessageAction {
                user_message: Some(pb::UserMessage {
                    message_id: "user-message".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
    .encode_to_vec();

    let mut request = Vec::new();
    prost::encoding::encode_key(1, WireType::LengthDelimited, &mut request);
    prost::encoding::encode_varint((state.len() + extra) as u64, &mut request);
    request.extend_from_slice(&state);
    push_len_delimited(&mut request, 2, &action);
    push_len_delimited(&mut request, 5, CONVERSATION_ID.as_bytes());

    let mut payload = Vec::new();
    push_len_delimited(&mut payload, 1, &request);
    payload
}

/// A state whose field stream is misaligned at a field boundary: the key byte
/// carries wire type 6, so decoding fails while merging `conversation_state` and
/// the report names no state field — the shape reported after v1.0.5 shipped.
fn invalid_wire_type_payload() -> Vec<u8> {
    misaligned_state_payload(&[0x0e, 0x00, 0x00, 0x00], 6, b"summary-bytes")
}

/// The same misalignment landing on a named field: a key for tag 7 (`plan`) with
/// wire type 1, reported as `ConversationStateStructure.plan: invalid wire type:
/// SixtyFourBit (expected LengthDelimited)`.
fn misaligned_plan_payload() -> Vec<u8> {
    misaligned_state_payload(&[0x39, 0, 0, 0, 0, 0, 0, 0, 0], 18, b"src/a.ts")
}

fn misaligned_state_payload(junk: &[u8], tag: u32, trailing: &[u8]) -> Vec<u8> {
    let mut state = Vec::new();
    push_len_delimited(&mut state, 8, b"turn-1");
    state.extend_from_slice(junk);
    push_len_delimited(&mut state, tag, trailing);

    let action = pb::ConversationAction {
        action: Some(pb::conversation_action::Action::UserMessageAction(
            pb::UserMessageAction {
                user_message: Some(pb::UserMessage {
                    message_id: "user-message".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
        )),
        ..Default::default()
    }
    .encode_to_vec();

    let mut request = Vec::new();
    push_len_delimited(&mut request, 1, &state);
    push_len_delimited(&mut request, 2, &action);
    let mut payload = Vec::new();
    push_len_delimited(&mut payload, 1, &request);
    payload
}

fn append_request(payload: Vec<u8>) -> ai::BidiAppendRequest {
    ai::BidiAppendRequest {
        request_id: Some(ai::BidiRequestId {
            request_id: "corrupt-state-append".into(),
        }),
        append_seqno: 1,
        data_binary: payload,
        ..Default::default()
    }
}

/// The fixture really is malformed protobuf, not a test artefact.
#[test]
fn corrupt_append_payload_fails_strict_decoding() {
    let error = pb::AgentClientMessage::decode(corrupt_append_payload().as_slice())
        .expect_err("the fixture must be rejected by a strict decode");
    assert!(
        error.to_string().contains("delimited length exceeded"),
        "unexpected strict decode error: {error}"
    );
}

#[test]
fn valid_append_payload_decodes_unchanged() {
    let decoded = bidi::decode(&append_request(valid_append_payload()), false)
        .unwrap()
        .into_agent();
    assert_eq!(decoded.conversation_id(), Some(CONVERSATION_ID));
    assert_eq!(decoded.model_id(), Some(MODEL_ID));
}

#[test]
fn append_survives_a_corrupt_file_state_entry() {
    let decoded = bidi::decode(&append_request(corrupt_append_payload()), false)
        .expect("a corrupt file state must not fail the whole append")
        .into_agent();

    assert_eq!(decoded.conversation_id(), Some(CONVERSATION_ID));
    assert_eq!(decoded.model_id(), Some(MODEL_ID));

    let Some(pb::agent_client_message::Message::RunRequest(run)) = decoded.message.message else {
        panic!("expected a run request");
    };
    let state = run
        .conversation_state
        .expect("the salvageable checkpoint state must survive");
    assert_eq!(state.turns, state_turns());
    assert_eq!(state.summary, Some(b"summary-bytes".to_vec()));
    assert_eq!(
        state.file_states.get(KEEP_PATH).map(Vec::as_slice),
        Some(b"keep".as_slice()),
        "readable file states must be kept"
    );
    assert!(
        !state.file_states.contains_key(DROP_PATH),
        "the corrupt file state must be dropped instead of poisoning the state"
    );
}

#[test]
fn append_survives_corruption_in_the_middle_of_the_state() {
    let decoded = bidi::decode(&append_request(mid_state_corrupt_payload()), false)
        .expect("a corrupt file state must not fail the whole append")
        .into_agent();
    let Some(pb::agent_client_message::Message::RunRequest(run)) = decoded.message.message else {
        panic!("expected a run request");
    };
    let state = run
        .conversation_state
        .expect("the readable part of the state must survive");
    assert_eq!(
        state.turns,
        state_turns(),
        "fields before the damage must be kept"
    );
    assert_eq!(
        state.summary, None,
        "fields after the damage are unreadable and must be dropped"
    );
}

#[test]
fn append_survives_a_misaligned_plan_key_inside_the_state() {
    let payload = misaligned_plan_payload();
    let error = pb::AgentClientMessage::decode(payload.as_slice())
        .expect_err("the fixture must reproduce the reported failure");
    assert_eq!(
        error.to_string(),
        "failed to decode Protobuf message: ConversationStateStructure.plan: \
         AgentRunRequest.conversation_state: AgentClientMessage.message: \
         invalid wire type: SixtyFourBit (expected LengthDelimited)"
    );

    let decoded = bidi::decode(&append_request(payload), false)
        .expect("the append must be recovered")
        .into_agent();
    let Some(pb::agent_client_message::Message::RunRequest(run)) = decoded.message.message else {
        panic!("expected a run request");
    };
    assert!(run.action.is_some(), "the action must survive");
    let state = run.conversation_state.expect("the state must be kept");
    assert_eq!(state.turns, vec![b"turn-1".to_vec()]);
    assert_eq!(state.plan, None, "the misaligned tail is unreadable");
}

#[test]
fn append_survives_an_invalid_wire_type_inside_the_state() {
    let payload = invalid_wire_type_payload();
    let error = pb::AgentClientMessage::decode(payload.as_slice())
        .expect_err("the fixture must reproduce the reported failure");
    assert_eq!(
        error.to_string(),
        "failed to decode Protobuf message: AgentRunRequest.conversation_state: \
         AgentClientMessage.message: invalid wire type value: 6"
    );

    let decoded = bidi::decode(&append_request(payload), false)
        .expect("the append must be recovered")
        .into_agent();
    let Some(pb::agent_client_message::Message::RunRequest(run)) = decoded.message.message else {
        panic!("expected a run request");
    };
    assert!(run.action.is_some(), "the action must survive");
    let state = run.conversation_state.expect("the state must be kept");
    assert_eq!(state.turns, vec![b"turn-1".to_vec()]);
    assert_eq!(
        state.summary, None,
        "everything after the misaligned key is unreadable"
    );
}

#[test]
fn append_survives_a_state_length_that_swallows_the_action() {
    for extra in [1usize, 2, 4, 8, 32] {
        let payload = overlong_state_payload(extra);
        assert!(
            pb::AgentClientMessage::decode(payload.as_slice()).is_err(),
            "the fixture must be corrupt (extra={extra})"
        );
        let decoded = bidi::decode(&append_request(payload), false)
            .unwrap_or_else(|error| panic!("extra={extra}: {error}"))
            .into_agent();
        assert_eq!(
            decoded.conversation_id(),
            Some(CONVERSATION_ID),
            "extra={extra}"
        );
        let Some(pb::agent_client_message::Message::RunRequest(run)) = decoded.message.message
        else {
            panic!("expected a run request (extra={extra})")
        };
        assert!(
            run.action.is_some(),
            "the action must survive a state length that overshot (extra={extra})"
        );
        let state = run
            .conversation_state
            .expect("the readable part of the state must be kept");
        assert_eq!(state.turns, state_turns(), "extra={extra}");
    }
}

#[test]
fn rejected_payloads_are_written_out_for_diagnosis() {
    let directory = tempfile::tempdir().unwrap();
    let payload = vec![0xff, 0xff, 0xff, 0xff];
    let path = salvage::dump_rejected(directory.path(), "request-id", &payload)
        .expect("a rejected payload must be written out");
    assert_eq!(std::fs::read(&path).unwrap(), payload);
    assert!(path.ends_with("request-id.bin"));
    assert!(salvage::dump_rejected(directory.path(), "empty", &[]).is_none());
}

#[test]
fn salvage_reports_the_fields_it_dropped() {
    let salvaged = salvage::salvage_fields(&corrupt_append_payload()).unwrap();
    assert_eq!(
        salvaged.dropped,
        vec![
            "ConversationStateStructure#12".to_string(),
            "ConversationStateStructure#tail(1B)".to_string(),
        ],
        "the log line has to name what was lost"
    );
}

#[test]
fn corrupt_run_request_fields_stay_a_protocol_error() {
    // Only the opaque state is salvageable. A corrupt action is what the run is
    // made of, so the append must fail instead of starting a half-read turn.
    let run_request = pb::AgentRunRequest {
        conversation_id: Some(CONVERSATION_ID.into()),
        ..Default::default()
    };
    let mut request_bytes = run_request.encode_to_vec();
    let mut action = pb::ConversationAction {
        action: Some(pb::conversation_action::Action::CancelAction(
            pb::CancelAction::default(),
        )),
        ..Default::default()
    }
    .encode_to_vec();
    action.push(0x07); // tag 0 is invalid
    push_len_delimited(&mut request_bytes, 2, &action);

    let mut payload = Vec::new();
    push_len_delimited(&mut payload, 1, &request_bytes);
    let Err(error) = bidi::decode(&append_request(payload), false) else {
        panic!("a corrupt action must fail the append");
    };
    assert!(
        error.to_string().contains("protobuf decode error"),
        "unexpected error: {error}"
    );
}

#[test]
fn truncated_payload_stays_a_protocol_error() {
    // Transport loss leaves bytes that no longer describe the message, so the
    // surviving prefix must not be turned into a half-read run request.
    let mut payload = valid_append_payload();
    payload.truncate(payload.len() - 3);
    assert!(bidi::decode(&append_request(payload), false).is_err());
}

#[test]
fn unreadable_envelope_stays_a_protocol_error() {
    let request = append_request(vec![0xff, 0xff, 0xff, 0xff]);
    assert!(bidi::decode(&request, false).is_err());
}
