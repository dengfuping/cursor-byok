//! Accepts ordered Cursor Bidi append requests and routes them by request_id.
use crate::{
    cursor::{
        conversation::TransportCommand,
        protocol::{
            events, inference,
            proto::{agent::v1 as agent, aiserver::v1 as ai},
            salvage,
        },
        transport::{TransportParent, TransportRegistry},
    },
    Error, Result,
};

pub struct DecodedAppend {
    pub request_id: String,
    pub seqno: i64,
    pub message: agent::AgentClientMessage,
}

impl DecodedAppend {
    pub fn model_id(&self) -> Option<&str> {
        let agent::agent_client_message::Message::RunRequest(request) =
            self.message.message.as_ref()?
        else {
            return None;
        };
        request
            .requested_model
            .as_ref()
            .map(|model| model.model_id.as_str())
            .filter(|model| !model.is_empty())
            .or_else(|| {
                request
                    .model_details
                    .as_ref()
                    .map(|model| model.model_id.as_str())
                    .filter(|model| !model.is_empty())
            })
    }

    pub fn conversation_id(&self) -> Option<&str> {
        let agent::agent_client_message::Message::RunRequest(request) =
            self.message.message.as_ref()?
        else {
            return None;
        };
        request.conversation_id.as_deref()
    }

    pub fn is_background_task_completion(&self) -> bool {
        let Some(agent::agent_client_message::Message::RunRequest(request)) =
            self.message.message.as_ref()
        else {
            return false;
        };
        matches!(
            request
                .action
                .as_ref()
                .and_then(|action| action.action.as_ref()),
            Some(agent::conversation_action::Action::BackgroundTaskCompletionAction(_))
        )
    }

    pub fn trace_metadata(&self) -> serde_json::Value {
        let Some(message) = self.message.message.as_ref() else {
            return serde_json::json!({
                "append_seqno": self.seqno,
                "message_type": "empty",
            });
        };
        let agent::agent_client_message::Message::RunRequest(request) = message else {
            return serde_json::json!({
                "append_seqno": self.seqno,
                "message_type": client_message_type(message),
            });
        };
        let (action_type, history_messages, history_images) = request
            .action
            .as_ref()
            .and_then(|action| action.action.as_ref())
            .map(|action| match action {
                agent::conversation_action::Action::UserMessageAction(action) => {
                    let history = action.conversation_history.as_ref();
                    (
                        "user_message",
                        history.map_or(0, |history| history.messages.len()),
                        history.map_or(0, history_image_count),
                    )
                }
                agent::conversation_action::Action::BackgroundTaskCompletionAction(_) => {
                    ("background_task_completion", 0, 0)
                }
                agent::conversation_action::Action::ExecutePlanAction(_) => ("execute_plan", 0, 0),
                agent::conversation_action::Action::SummarizeAction(_) => ("summarize", 0, 0),
                _ => ("other", 0, 0),
            })
            .unwrap_or(("none", 0, 0));
        let state = request.conversation_state.as_ref();
        serde_json::json!({
            "append_seqno": self.seqno,
            "message_type": "run_request",
            "conversation_id": request.conversation_id,
            "model_id": self.model_id(),
            "action_type": action_type,
            "conversation_history_messages": history_messages,
            "conversation_history_images": history_images,
            "root_message_count": state.map_or(0, |state| state.root_prompt_messages_json.len()),
            "turn_count": state.map_or(0, |state| state.turns.len()),
            "prefetched_blob_count": request.pre_fetched_blobs.len(),
        })
    }
}

fn client_message_type(message: &agent::agent_client_message::Message) -> &'static str {
    use agent::agent_client_message::Message;
    match message {
        Message::RunRequest(_) => "run_request",
        Message::ExecClientMessage(_) => "exec_client_message",
        Message::ExecClientControlMessage(_) => "exec_client_control_message",
        Message::KvClientMessage(_) => "kv_client_message",
        Message::ConversationAction(_) => "conversation_action",
        Message::InteractionResponse(_) => "interaction_response",
        Message::ClientHeartbeat(_) => "client_heartbeat",
        Message::PrewarmRequest(_) => "prewarm_request",
    }
}

fn history_image_count(history: &agent::ConversationHistory) -> usize {
    use agent::{
        conversation_history_message::Message,
        conversation_history_tool_result_content::Content as ToolContent,
        conversation_history_user_content::Content as UserContent,
    };
    history
        .messages
        .iter()
        .map(|message| match message.message.as_ref() {
            Some(Message::User(user)) => user
                .content
                .iter()
                .filter(|content| matches!(content.content, Some(UserContent::Image(_))))
                .count(),
            Some(Message::Tool(tool)) => tool
                .content
                .iter()
                .filter(|content| matches!(content.content, Some(ToolContent::Image(_))))
                .count(),
            _ => 0,
        })
        .sum()
}

pub fn decode(request: &ai::BidiAppendRequest) -> Result<DecodedAppend> {
    let request_id = request_id(request)?;
    let payload = payload(request)?;
    if let Some(frame) = inference::classify(&payload) {
        return Err(reject_run_inference(request_id, &payload, frame));
    }
    Ok(DecodedAppend {
        request_id: request_id.into(),
        seqno: request.append_seqno,
        message: salvage::decode(&payload)?,
    })
}

/// Keeps the bytes of an append that could not be decoded, so the next report of
/// a corrupt state can be diagnosed from data instead of inference.
///
/// Called by the HTTP handler rather than by `decode`, so embedding the ingest
/// path in a test never writes into the user's data directory.
pub fn report_rejection(request: &ai::BidiAppendRequest, error: &Error) {
    let Ok(payload) = payload(request) else {
        return;
    };
    let request_id = request_id(request).unwrap_or_default();
    let dump = crate::config::managed_data_dir()
        .ok()
        .map(|directory| directory.join("rejected"))
        .and_then(|directory| salvage::dump_rejected(&directory, request_id, &payload));
    tracing::warn!(
        request_id,
        bytes = payload.len(),
        dump = dump.as_ref().map(|path| path.display().to_string()),
        %error,
        "rejected a Cursor append that could not be decoded"
    );
}

/// Cursor's newer agent loop sends `aiserver.v1.InferenceService` frames instead
/// of `agent.v1.AgentRunRequest`. This server does not implement that protocol
/// yet; report it precisely instead of failing as a corrupt conversation state.
fn reject_run_inference(request_id: &str, payload: &[u8], frame: inference::Frame) -> Error {
    let described = match frame {
        inference::Frame::RunRequest(request) => {
            let text = inference::routing_text(&request);
            format!(
                "run_request {} routing_text_parts={}",
                inference::describe(&request),
                text.len()
            )
        }
        inference::Frame::InvokeModel(_) => "invoke_model".to_string(),
        inference::Frame::FinishRun(_) => "finish_run".to_string(),
        inference::Frame::CancelInvocation(_) => "cancel_invocation".to_string(),
        inference::Frame::Heartbeat => "heartbeat".to_string(),
        inference::Frame::Other(kind) => kind.to_string(),
    };
    let dump = crate::config::managed_data_dir()
        .ok()
        .map(|directory| directory.join("rejected"))
        .and_then(|directory| salvage::dump_rejected(&directory, request_id, payload));
    tracing::warn!(
        request_id,
        frame = described,
        dump = dump.as_ref().map(|path| path.display().to_string()),
        "received a Cursor RunInference frame; the protocol is not implemented yet"
    );
    Error::Protocol(
        "Cursor switched this run to aiserver RunInference, which cursor-byok does not support yet"
            .into(),
    )
}

fn request_id(request: &ai::BidiAppendRequest) -> Result<&str> {
    request
        .request_id
        .as_ref()
        .map(|id| id.request_id.as_str())
        .filter(|id| !id.is_empty())
        .ok_or_else(|| Error::Protocol("BidiAppend request_id is required".into()))
}

fn payload(request: &ai::BidiAppendRequest) -> Result<Vec<u8>> {
    if !request.data.is_empty() {
        return hex::decode(&request.data)
            .map_err(|error| Error::Protocol(format!("invalid BidiAppend hex: {error}")));
    }
    if !request.data_binary.is_empty() {
        return Ok(request.data_binary.clone());
    }
    Err(Error::Protocol(
        "BidiAppend contains no AgentClientMessage".into(),
    ))
}

pub async fn append(
    registry: &TransportRegistry,
    request: DecodedAppend,
    parent: Option<TransportParent>,
) -> Result<ai::BidiAppendResponse> {
    let replace_closing = request.model_id().is_some();
    let handle = registry
        .get_or_create_for_append(&request.request_id, replace_closing)
        .await?;
    let _admission = handle.admit()?;
    if let Some(conversation_id) = request.conversation_id() {
        handle.set_conversation_id(conversation_id)?;
    }
    if let Some(parent) = parent {
        handle.set_parent(parent)?;
    }
    if matches!(
        request.message.message.as_ref(),
        Some(agent::agent_client_message::Message::ClientHeartbeat(_))
    ) {
        handle.emit(&events::heartbeat())?;
    }
    handle
        .command(TransportCommand::Append {
            seqno: request.seqno,
            message: Box::new(request.message),
        })
        .await?;
    Ok(ai::BidiAppendResponse {})
}
