//! Recovers readable fields from Cursor payloads that prost rejects.
//!
//! The Cursor client owns `ConversationStateStructure`. This server never reads
//! it: it stores the state and echoes it back inside checkpoint updates. A client
//! that grows a value without rewriting the enclosing length prefix still produces
//! bytes prost rejects, and failing the append then costs the whole turn even
//! though nothing the server consumes was damaged.
//!
//! Salvage keeps every field that decodes on its own and descends through the two
//! envelope levels that wrap the state (`AgentClientMessage.message` and
//! `AgentRunRequest.conversation_state`). Nothing else is repaired: a corrupt
//! action or model field stays a protocol error, because a run started on a
//! half-read request is worse than a failed append.
use prost::{
    encoding::{self, WireType},
    Message,
};

use crate::{cursor::protocol::proto::agent::v1 as pb, Result};

/// Decodes an `AgentClientMessage`, dropping unreadable state fields if needed.
pub fn decode(payload: &[u8]) -> Result<pb::AgentClientMessage> {
    let error = match pb::AgentClientMessage::decode(payload) {
        Ok(message) => return Ok(message),
        Err(error) => error,
    };
    let Some(salvaged) = agent_client_message(payload) else {
        return Err(error.into());
    };
    let message = pb::AgentClientMessage::decode(salvaged.payload.as_slice())?;
    tracing::warn!(
        decode_error = %error,
        dropped = ?salvaged.dropped,
        "recovered a Cursor AgentClientMessage after a protobuf decode failure"
    );
    Ok(message)
}

/// A payload re-encoded without the fields that failed to decode.
pub struct Salvaged {
    pub payload: Vec<u8>,
    /// Message names and tags of the dropped fields, for logging.
    pub dropped: Vec<String>,
}

/// Rewrites an `AgentClientMessage` payload into its longest decodable form.
///
/// Returns `None` when the outer message cannot be split into fields at all.
pub fn agent_client_message(payload: &[u8]) -> Option<Salvaged> {
    let mut dropped = Vec::new();
    let payload = rebuild(Level::Client, payload, &mut dropped)?;
    Some(Salvaged { payload, dropped })
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
            Some(value) => {
                encoding::encode_key(field.tag, WireType::LengthDelimited, &mut output);
                encoding::encode_varint(value.len() as u64, &mut output);
                output.extend_from_slice(&value);
            }
            None if level.drops_fields() => {
                dropped.push(format!("{}#{}", level.message(), field.tag));
            }
            None => return None,
        }
    }
    if unparsed > 0 {
        dropped.push(format!("{}#tail({unparsed}B)", level.message()));
    }
    Some(output)
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
