//! Runs `aiserver.v1.InferenceService` invocations against configured providers.
//!
//! The agent host sends the whole model request in `invoke_model` (messages plus
//! tool definitions) and expects the answer as `invocation_response` frames, so
//! this path maps that request onto the same provider adapters the `agent.v1`
//! loop uses and streams the events back.
use std::sync::Arc;

use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use crate::{
    cursor::{protocol::proto::inference::v1 as pb, transport::InferenceHub},
    model::{
        ContentPart, ModelInvocation, ModelRequest, ModelSpec, ProjectedContent, ProjectedMessage,
        PromptSpec, Role,
    },
    provider::{ModelEvent, Provider},
    store::Store,
    Error, Result,
};

use pb::inference_stream_response::Response;
use pb::run_inference_server_message::Message as Server;

/// One model call requested by the agent host.
pub struct Invocation {
    pub request_id: String,
    pub invocation_id: String,
    pub conversation_id: String,
    /// Model named in `run_ready`; `invoke_model` carries no model of its own.
    pub resolved_model: Option<String>,
    pub request: pb::InferenceStreamRequest,
}

/// Streams the provider answer for one invocation back to the agent host.
pub async fn run(
    store: Store,
    provider: Arc<dyn Provider>,
    hub: InferenceHub,
    invocation: Invocation,
) -> Result<()> {
    let model_id = requested_model(&invocation.request)
        .or(invocation.resolved_model.clone())
        .ok_or_else(|| Error::Protocol("RunInference invocation does not select a model".into()))?;
    // The agent host asks for `default` when the conversation has no explicit
    // model; BYOK answers with the first configured model, exactly the one the
    // desktop app lists first.
    let config = match store.model(&model_id).await? {
        Some(config) => config,
        None if model_id == "default" => store
            .models()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Config("no BYOK model is configured".into()))?,
        None => return Err(Error::Protocol(format!("unknown BYOK model: {model_id}"))),
    };
    tracing::info!(
        model = %config.model_id,
        name = %config.display_name,
        "run inference resolved model"
    );
    // The provider router resolves a model by its stored hash; the upstream model
    // id is filled in from that record.
    let mut model = ModelSpec::new(config.model_hash.clone());
    config.configure(&mut model);

    let (instructions, history) = convert_messages(&invocation.request.messages);
    let request = ModelRequest {
        prompt: PromptSpec {
            instructions,
            // Tool definitions are mapped once tool calls are implemented; with
            // none advertised the model answers with text only.
            tools: Vec::new(),
        },
        model,
        history,
    };
    let model_invocation = ModelInvocation {
        call_id: invocation.invocation_id.clone(),
        run_id: invocation.request_id.clone(),
        conversation_id: invocation.conversation_id.clone(),
        provider_call_index: 0,
        request,
    };

    let request_id = invocation.request_id.as_str();
    let invocation_id = invocation.invocation_id.clone();
    publish(
        &hub,
        request_id,
        &invocation_id,
        invocation_id_response(&invocation_id),
    )
    .await;

    let mut stream = provider.stream(model_invocation, CancellationToken::new());
    let mut completion_tokens = 0_i32;
    let mut prompt_tokens = 0_i32;
    while let Some(event) = stream.next().await {
        match event {
            Ok(ModelEvent::TextDelta(text)) => {
                completion_tokens += estimate_tokens(&text);
                publish(
                    &hub,
                    request_id,
                    &invocation_id,
                    text_response(&text, false),
                )
                .await;
            }
            Ok(ModelEvent::ThinkingDelta(text)) => {
                publish(&hub, request_id, &invocation_id, thinking_response(&text)).await;
            }
            Ok(ModelEvent::Usage(usage)) => {
                prompt_tokens = usage.input_tokens.unwrap_or_default() as i32;
                completion_tokens = usage.output_tokens.unwrap_or_default() as i32;
            }
            Ok(ModelEvent::Done(_)) => break,
            Ok(_) => {}
            Err(error) => {
                publish(
                    &hub,
                    request_id,
                    &invocation_id,
                    error_response(&error.to_string()),
                )
                .await;
                return Err(error);
            }
        }
    }

    publish(&hub, request_id, &invocation_id, text_response("", true)).await;
    publish(
        &hub,
        request_id,
        &invocation_id,
        usage_response(prompt_tokens, completion_tokens),
    )
    .await;
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
    Ok(())
}

fn requested_model(request: &pb::InferenceStreamRequest) -> Option<String> {
    request
        .model_id
        .clone()
        .filter(|model| !model.is_empty())
        .or_else(|| {
            request
                .requested_model
                .as_ref()
                .map(|model| model.model_id.clone())
                .filter(|model| !model.is_empty())
        })
}

/// Splits the client's messages into system instructions and provider history.
fn convert_messages(messages: &[pb::InferenceCoreMessage]) -> (String, Vec<ProjectedMessage>) {
    let mut instructions = String::new();
    let mut history = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        let role = pb::InferenceMessageRole::try_from(message.role).unwrap_or_default();
        let text = message_text(message);
        match role {
            pb::InferenceMessageRole::System => {
                if !text.is_empty() {
                    if !instructions.is_empty() {
                        instructions.push_str("\n\n");
                    }
                    instructions.push_str(&text);
                }
            }
            pb::InferenceMessageRole::Assistant => history.push(ProjectedMessage {
                message_id: format!("inference:{index}"),
                role: Role::Assistant,
                content: ProjectedContent::Assistant {
                    text,
                    thinking: String::new(),
                    replay_state: None,
                    calls: Vec::new(),
                },
            }),
            pb::InferenceMessageRole::Tool => history.push(ProjectedMessage {
                message_id: format!("inference:{index}"),
                role: Role::Tool,
                content: ProjectedContent::Parts(vec![ContentPart::Text { text }]),
            }),
            pb::InferenceMessageRole::User | pb::InferenceMessageRole::Unspecified => {
                history.push(ProjectedMessage {
                    message_id: format!("inference:{index}"),
                    role: Role::User,
                    content: ProjectedContent::Parts(vec![ContentPart::Text { text }]),
                })
            }
        }
    }
    (instructions, history)
}

fn message_text(message: &pb::InferenceCoreMessage) -> String {
    use pb::inference_core_message::Content;

    match message.content.as_ref() {
        Some(Content::Text(text)) => text.clone(),
        Some(Content::Parts(parts)) => parts
            .parts
            .iter()
            .filter_map(|part| match part.part.as_ref() {
                Some(pb::inference_content_part::Part::Text(text)) => Some(text.text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
        Some(Content::ToolContent(content)) => content
            .parts
            .iter()
            .map(|part| match part.result.as_ref() {
                Some(value) => format!("{}: {value:?}", part.tool_name),
                None => part.tool_name.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        None => message.assistant_text_parts.join(""),
    }
}

fn estimate_tokens(text: &str) -> i32 {
    (text.chars().count() / 4).max(1) as i32
}

async fn publish(
    hub: &InferenceHub,
    request_id: &str,
    invocation_id: &str,
    response: pb::InferenceStreamResponse,
) {
    let frame = pb::RunInferenceServerMessage {
        message: Some(Server::InvocationResponse(
            pb::RunInferenceInvocationResponse {
                invocation_id: invocation_id.into(),
                response: Some(response),
            },
        )),
    };
    if let Err(error) = hub.publish(request_id, &frame).await {
        tracing::warn!(request_id, %error, "cannot publish inference response");
    }
}

fn invocation_id_response(invocation_id: &str) -> pb::InferenceStreamResponse {
    pb::InferenceStreamResponse {
        response: Some(Response::InvocationId(pb::InferenceInvocationIdInfo {
            invocation_id: invocation_id.into(),
        })),
        ..Default::default()
    }
}

fn text_response(text: &str, is_final: bool) -> pb::InferenceStreamResponse {
    pb::InferenceStreamResponse {
        response: Some(Response::TextPart(pb::InferenceTextStreamPart {
            text: text.into(),
            is_final,
        })),
        ..Default::default()
    }
}

fn thinking_response(text: &str) -> pb::InferenceStreamResponse {
    pb::InferenceStreamResponse {
        response: Some(Response::ThinkingPart(pb::InferenceThinkingStreamPart {
            text: text.into(),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn usage_response(prompt_tokens: i32, completion_tokens: i32) -> pb::InferenceStreamResponse {
    pb::InferenceStreamResponse {
        response: Some(Response::Usage(pb::InferenceUsageInfo {
            prompt_tokens,
            completion_tokens,
            total_tokens: Some(prompt_tokens + completion_tokens),
        })),
        ..Default::default()
    }
}

fn error_response(message: &str) -> pb::InferenceStreamResponse {
    pb::InferenceStreamResponse {
        response: Some(Response::Error(pb::InferenceStreamError {
            message: message.into(),
            ..Default::default()
        })),
        ..Default::default()
    }
}
