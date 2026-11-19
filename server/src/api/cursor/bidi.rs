//! Accepts ordered Cursor Bidi append requests and routes them by request_id.
use crate::{
    cursor::{
        conversation::TransportCommand,
        protocol::{
            events, inference,
            proto::{agent::v1 as agent, aiserver::v1 as ai, inference::v1 as pb},
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

#[allow(clippy::large_enum_variant)]
pub enum Decoded {
    /// A legacy `agent.v1` append handled by the conversation runtime.
    Agent(DecodedAppend),
    /// An `aiserver.v1.InferenceService` frame handled by the inference path.
    Inference {
        request_id: String,
        frame: inference::Frame,
    },
}

/// The request id of an append, for callers that need it before decoding.
pub fn request_id_of(request: &ai::BidiAppendRequest) -> Result<&str> {
    request_id(request)
}

pub fn decode(request: &ai::BidiAppendRequest, inference_active: bool) -> Result<Decoded> {
    let request_id = request_id(request)?;
    let payload = payload(request)?;
    let inference_frame = inference::classify(&payload).or_else(|| {
        // Once a run uses the inference protocol, every later frame of that
        // request belongs to it; `finish_run` has no identifying field.
        inference_active
            .then(|| inference::decode(&payload))
            .flatten()
            .and_then(std::result::Result::ok)
    });
    if let Some(frame) = inference_frame {
        return Ok(Decoded::Inference {
            request_id: request_id.into(),
            frame,
        });
    }
    Ok(Decoded::Agent(DecodedAppend {
        request_id: request_id.into(),
        seqno: request.append_seqno,
        message: salvage::decode(&payload)?,
    }))
}

impl Decoded {
    /// The agent append, for callers that expect the legacy protocol.
    pub fn into_agent(self) -> DecodedAppend {
        match self {
            Self::Agent(decoded) => decoded,
            Self::Inference { frame, .. } => {
                panic!("expected an agent.v1 append, got an inference frame: {frame:?}")
            }
        }
    }
}

/// Answers the `aiserver.v1.InferenceService` frames the agent host sends.
///
/// `run_request` is acknowledged with `run_ready`; `invoke_model` is where the
/// model call belongs and currently answers with a marker text so the frame
/// plumbing can be verified end to end.
pub async fn handle_inference(
    registry: &TransportRegistry,
    request_id: &str,
    frame: inference::Frame,
) -> Result<()> {
    use pb::run_inference_server_message::Message as Server;

    let hub = registry.inference();
    match frame {
        inference::Frame::RunRequest(request) => {
            tracing::info!(
                request_id,
                run = %inference::describe(&request),
                "run inference started"
            );
            hub.publish(
                request_id,
                &pb::RunInferenceServerMessage {
                    message: Some(Server::RunReady(pb::RunInferenceRunReady {
                        resolved_model: request.requested_model.clone(),
                        // The agent host builds the prompt locally and refuses
                        // the turn without a model family: `metadata-unavailable`
                        // comes from an empty `prompt_model_metadata`.
                        prompt_model_metadata: Some(pb::RunInferencePromptModelMetadata {
                            vendor: prompt_vendor(&request).into(),
                            prompt_version: PROMPT_VERSION.into(),
                            ..Default::default()
                        }),
                        accepts_client_heartbeat: true,
                        ..Default::default()
                    })),
                },
            )
            .await?;
        }
        inference::Frame::InvokeModel(request) => {
            let context = request.request.as_ref();
            tracing::info!(
                request_id,
                invocation_id = request.invocation_id,
                messages = context.map_or(0, |context| context.messages.len()),
                tools = context.map_or(0, |context| context.tools.len()),
                model = context.and_then(|context| context.model_id.clone()),
                tool_batches = request.tool_call_batches.len(),
                "run inference invoke_model"
            );
            let invocation_id = request.invocation_id.clone();
            // The agent host tracks the model invocation id across the stream and
            // needs it echoed before any content; it also counts output tokens
            // from the usage part.
            let text = "cursor-byok: RunInference path reached";
            // The agent host maps `text_part` with `is_final` to a finish event and
            // only treats parts without it as content, so the text goes out as a
            // delta followed by an empty terminal part.
            for response in [
                invocation_id_response(&invocation_id),
                stream_response(text, false),
                stream_response("", true),
                usage_response(text),
            ] {
                hub.publish(
                    request_id,
                    &pb::RunInferenceServerMessage {
                        message: Some(Server::InvocationResponse(
                            pb::RunInferenceInvocationResponse {
                                invocation_id: invocation_id.clone(),
                                response: Some(response),
                            },
                        )),
                    },
                )
                .await?;
            }
            hub.publish(
                request_id,
                &pb::RunInferenceServerMessage {
                    message: Some(Server::InvocationEnd(pb::RunInferenceInvocationEnd {
                        invocation_id,
                        error: None,
                        delta_refused: None,
                    })),
                },
            )
            .await?;
        }
        inference::Frame::FinishRun(_) => tracing::info!(request_id, "run inference finish_run"),
        inference::Frame::CancelInvocation(frame) => tracing::info!(
            request_id,
            invocation_id = frame.invocation_id,
            "run inference cancel_invocation"
        ),
        inference::Frame::Heartbeat => {}
        inference::Frame::Other(kind) => tracing::debug!(request_id, frame = kind),
    }
    Ok(())
}

/// Prompt family the agent host should build for, derived from the model id.
///
/// The vendor only changes the prompt shape the client renders; BYOK models are
/// served by OpenAI- or Anthropic-compatible endpoints, so the vendor of the
/// configured model decides.
fn prompt_vendor(request: &pb::RunInferenceRunRequest) -> &'static str {
    let model = request
        .requested_model
        .as_ref()
        .map(|model| model.model_id.to_ascii_lowercase())
        .unwrap_or_default();
    if model.contains("claude") || model.contains("sonnet") || model.contains("opus") {
        "anthropic"
    } else if model.contains("gemini") {
        "google"
    } else {
        "openai"
    }
}

/// Prompt version the client knows; `cursor-0226` is what its own catalog sends.
const PROMPT_VERSION: &str = "cursor-0226";

fn stream_response(text: &str, is_final: bool) -> pb::InferenceStreamResponse {
    use pb::inference_stream_response::Response;

    pb::InferenceStreamResponse {
        response: Some(Response::TextPart(pb::InferenceTextStreamPart {
            text: text.into(),
            is_final,
        })),
        ..Default::default()
    }
}

/// Echoes the model invocation id the agent host expects to see first.
fn invocation_id_response(invocation_id: &str) -> pb::InferenceStreamResponse {
    use pb::inference_stream_response::Response;

    pb::InferenceStreamResponse {
        response: Some(Response::InvocationId(pb::InferenceInvocationIdInfo {
            invocation_id: invocation_id.into(),
        })),
        ..Default::default()
    }
}

/// Rough token accounting so the agent host does not classify the turn as empty.
fn usage_response(text: &str) -> pb::InferenceStreamResponse {
    use pb::inference_stream_response::Response;

    let completion = (text.chars().count() / 4).max(1) as i32;
    pb::InferenceStreamResponse {
        response: Some(Response::Usage(pb::InferenceUsageInfo {
            prompt_tokens: 0,
            completion_tokens: completion,
            total_tokens: Some(completion),
        })),
        ..Default::default()
    }
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
