//! Recovers readable fields from Cursor payloads that prost rejects.
//!
//! The Cursor client owns `ConversationStateStructure`. This server never reads
//! it: it stores the state and echoes it back inside checkpoint updates. Two
//! client-side defects still make that opaque blob decide whether a turn runs:
//!
//! - a nested length prefix that disagrees with the field it wraps, and
//! - a `conversation_state` field length that runs past the serialized state into
//!   the fields that follow it, which swallows `action`.
//!
//! Both are repaired here instead of failing the append. Only the state may lose
//! bytes: everything the run is built from is either kept byte for byte or the
//! append stays a protocol error.
use std::path::{Path, PathBuf};

use prost::{
    encoding::{self, WireType},
    Message,
};

use crate::{cursor::protocol::proto::agent::v1 as pb, Result};

const MAX_DUMP_BYTES: usize = 64 * 1024 * 1024;

/// Decodes an `AgentClientMessage`, repairing unreadable state fields if needed.
pub fn decode(payload: &[u8]) -> Result<pb::AgentClientMessage> {
    let error = match pb::AgentClientMessage::decode(payload) {
        Ok(message) => return Ok(message),
        Err(error) => error,
    };
    let mut fallback = None;
    for attempt in [salvage_fields, salvage_resync] {
        let Some(salvaged) = attempt(payload) else {
            continue;
        };
        let Some(message) = pb::AgentClientMessage::decode(salvaged.payload.as_slice()).ok() else {
            continue;
        };
        // A repaired request that lost its action is not a repair, so prefer the
        // attempt that still describes the turn. Without one, keep the first
        // readable result rather than refusing a request that never had an action.
        if has_action(&message) {
            recovered(&error, &salvaged.dropped);
            return Ok(message);
        }
        fallback.get_or_insert((salvaged.dropped, message));
    }
    if let Some((dropped, message)) = fallback {
        recovered(&error, &dropped);
        return Ok(message);
    }
    Err(error.into())
}

/// Writes a rejected append next to the database so it can be inspected later.
pub fn dump_rejected(directory: &Path, request_id: &str, payload: &[u8]) -> Option<PathBuf> {
    if payload.is_empty() || payload.len() > MAX_DUMP_BYTES {
        return None;
    }
    std::fs::create_dir_all(directory).ok()?;
    let path = directory.join(format!("{request_id}.bin"));
    std::fs::write(&path, payload).ok()?;
    Some(path)
}

/// A payload re-encoded without the fields that failed to decode.
pub struct Salvaged {
    pub payload: Vec<u8>,
    /// Message names and tags of the dropped fields, for logging.
    pub dropped: Vec<String>,
}

/// Drops every field inside `conversation_state` that cannot be decoded.
pub fn salvage_fields(payload: &[u8]) -> Option<Salvaged> {
    let mut dropped = Vec::new();
    let payload = rebuild(Level::Client, payload, &mut dropped)?;
    Some(Salvaged { payload, dropped })
}

/// Finds where the state really ends when its length prefix overshoots it.
pub fn salvage_resync(payload: &[u8]) -> Option<Salvaged> {
    let mut dropped = Vec::new();
    let payload = resync(payload, &mut dropped)?;
    Some(Salvaged { payload, dropped })
}

/// Whether the decoded message still carries the turn it was sent for.
fn has_action(message: &pb::AgentClientMessage) -> bool {
    match message.message.as_ref() {
        Some(pb::agent_client_message::Message::RunRequest(request)) => request.action.is_some(),
        _ => true,
    }
}

fn recovered(error: &prost::DecodeError, dropped: &[String]) {
    tracing::warn!(
        decode_error = %error,
        dropped = ?dropped,
        "recovered a Cursor AgentClientMessage after a protobuf decode failure"
    );
}

#[derive(Clone, Copy)]
enum Level {
    Client,
    RunRequest,
    State,
}

impl Level {
    fn message(self) -> &'static str {
        match self {
            Self::Client => "AgentClientMessage",
            Self::RunRequest => "AgentRunRequest",
            Self::State => "ConversationStateStructure",
        }
    }

    /// Whether one encoded field decodes on its own at this level.
    fn decodes(self, field: &[u8]) -> bool {
        match self {
            Self::Client => pb::AgentClientMessage::decode(field).is_ok(),
            Self::RunRequest => pb::AgentRunRequest::decode(field).is_ok(),
            Self::State => pb::ConversationStateStructure::decode(field).is_ok(),
        }
    }

    /// The nested message carried by `tag`, when it is one this module repairs.
    fn nested(self, tag: u32) -> Option<Self> {
        match (self, tag) {
            (Self::Client, 1) => Some(Self::RunRequest),
            (Self::RunRequest, 1) => Some(Self::State),
            _ => None,
        }
    }

    /// Only the opaque state may lose fields. Everything the server consumes has
    /// to survive, otherwise the append stays a protocol error.
    fn drops_fields(self) -> bool {
        matches!(self, Self::State)
    }
}

fn rebuild(level: Level, payload: &[u8], dropped: &mut Vec<String>) -> Option<Vec<u8>> {
    let (fields, unparsed) = match level {
        // A bad nested length consumes its own tail, so the state is the only
        // level where the readable fields cannot be enumerated exactly.
        Level::State => split_prefix(payload),
        _ => (split(payload)?, 0),
    };
    let mut output = Vec::with_capacity(payload.len());
    for field in fields {
        if level.decodes(field.raw) {
            output.extend_from_slice(field.raw);
            continue;
        }
        let rebuilt = field
            .nested(level)
            .and_then(|nested| rebuild(nested, field.value, dropped));
        match rebuilt {
            Some(value) => encode_field(&mut output, field.tag, &value),
            None if level.drops_fields() => {
                dropped.push(format!("{}#{}", level.message(), field.tag));
            }
            // Keep unreadable fields exactly as they are: the final decode then
            // reports the real problem instead of a state field it never touched.
            None => {
                dropped.push(format!("{}#{} kept", level.message(), field.tag));
                output.extend_from_slice(field.raw);
            }
        }
    }
    if unparsed > 0 {
        dropped.push(format!("{}#tail({unparsed}B)", level.message()));
    }
    Some(output)
}

/// Recovers a run request whose `conversation_state` length overshoots the state.
///
/// The bytes after the real state are `action` and the rest of the request, but
/// their boundary is lost. The true boundary is one of the state's own field
/// boundaries, so those are the candidates; one is accepted when the bytes that
/// follow it parse as the rest of a run request, action first.
fn resync(payload: &[u8], dropped: &mut Vec<String>) -> Option<Vec<u8>> {
    let envelope = split(payload)?;
    let [message_field] = envelope.as_slice() else {
        return None;
    };
    if message_field.tag != 1 || !matches!(message_field.wire_type, WireType::LengthDelimited) {
        return None;
    }
    let request = message_field.value;
    let mut offset = 0;
    while offset < request.len() {
        let field_start = offset;
        let header = header(request, offset)?;
        offset = header.end;
        if header.tag != 1 || !matches!(header.wire_type, WireType::LengthDelimited) {
            continue;
        }
        // Walk the candidates from the largest down: the declared end is the
        // right answer when the state itself is corrupt, and the true boundary
        // when the length prefix overshot. Either way it is the last one whose
        // remainder still reads as the rest of the request.
        for end in boundaries(request, header.value_start).into_iter().rev() {
            if end <= header.value_start
                || end > request.len()
                || !starts_with_action(&request[end..])
            {
                continue;
            }
            if !remainder_decodes(&request[end..]) {
                continue;
            }
            let state = rebuild(Level::State, &request[header.value_start..end], dropped)?;
            let mut rebuilt = Vec::with_capacity(request.len());
            rebuilt.extend_from_slice(&request[..field_start]);
            encode_field(&mut rebuilt, 1, &state);
            rebuilt.extend_from_slice(&request[end..]);
            dropped.push(format!("AgentRunRequest#1@end={end}"));
            let mut output = Vec::with_capacity(payload.len());
            encode_field(&mut output, 1, &rebuilt);
            return Some(output);
        }
        return None;
    }
    None
}

/// Every field boundary inside a message region, including the declared end.
fn boundaries(payload: &[u8], start: usize) -> Vec<usize> {
    const MAX_BOUNDARIES: usize = 4096;
    let mut found = Vec::new();
    let mut offset = start;
    while offset < payload.len() && found.len() < MAX_BOUNDARIES {
        let Some(header) = header(payload, offset) else {
            break;
        };
        if header.end <= offset {
            break;
        }
        found.push(header.end.min(payload.len()));
        offset = header.end;
    }
    found
}

fn starts_with_action(rest: &[u8]) -> bool {
    header(rest, 0).is_some_and(|header| {
        header.tag == 2 && matches!(header.wire_type, WireType::LengthDelimited)
    })
}

fn remainder_decodes(rest: &[u8]) -> bool {
    let Some(fields) = split(rest) else {
        return false;
    };
    !fields.is_empty()
        && fields
            .iter()
            .all(|field| pb::AgentRunRequest::decode(field.raw).is_ok())
}

fn encode_field(output: &mut Vec<u8>, tag: u32, payload: &[u8]) {
    encoding::encode_key(tag, WireType::LengthDelimited, output);
    encoding::encode_varint(payload.len() as u64, output);
    output.extend_from_slice(payload);
}

struct Header {
    tag: u32,
    wire_type: WireType,
    value_start: usize,
    /// Offset just past the field, even when the declared length overshoots.
    end: usize,
}

fn header(payload: &[u8], offset: usize) -> Option<Header> {
    let mut buf = &payload[offset..];
    let (tag, wire_type) = encoding::decode_key(&mut buf).ok()?;
    if !matches!(wire_type, WireType::LengthDelimited) {
        encoding::skip_field(wire_type, tag, &mut buf, Default::default()).ok()?;
        let end = payload.len() - buf.len();
        return Some(Header {
            tag,
            wire_type,
            value_start: end,
            end,
        });
    }
    let declared = usize::try_from(encoding::decode_varint(&mut buf).ok()?).ok()?;
    let value_start = payload.len() - buf.len();
    Some(Header {
        tag,
        wire_type,
        value_start,
        end: value_start.saturating_add(declared),
    })
}

struct Field<'a> {
    tag: u32,
    wire_type: WireType,
    /// The field exactly as it appeared, key included.
    raw: &'a [u8],
    /// The payload of a length-delimited field, empty otherwise.
    value: &'a [u8],
}

impl Field<'_> {
    fn nested(&self, level: Level) -> Option<Level> {
        if !matches!(self.wire_type, WireType::LengthDelimited) {
            return None;
        }
        level.nested(self.tag)
    }
}

/// Splits a message into the fields it contains.
fn split(buf: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut fields = Vec::new();
    let mut rest = buf;
    while !rest.is_empty() {
        let (field, tail) = field(rest)?;
        fields.push(field);
        rest = tail;
    }
    Some(fields)
}

/// Splits as many leading fields as possible and reports the unreadable tail.
fn split_prefix(buf: &[u8]) -> (Vec<Field<'_>>, usize) {
    let mut fields = Vec::new();
    let mut rest = buf;
    while !rest.is_empty() {
        let Some((field, tail)) = field(rest) else {
            break;
        };
        fields.push(field);
        rest = tail;
    }
    (fields, rest.len())
}

fn field(mut buf: &[u8]) -> Option<(Field<'_>, &[u8])> {
    let start = buf;
    let (tag, wire_type) = encoding::decode_key(&mut buf).ok()?;
    let value = if matches!(wire_type, WireType::LengthDelimited) {
        let len = usize::try_from(encoding::decode_varint(&mut buf).ok()?).ok()?;
        let (value, rest) = buf.split_at_checked(len)?;
        buf = rest;
        value
    } else {
        encoding::skip_field(wire_type, tag, &mut buf, Default::default()).ok()?;
        &[]
    };
    Some((
        Field {
            tag,
            wire_type,
            raw: &start[..start.len() - buf.len()],
            value,
        },
        buf,
    ))
}
