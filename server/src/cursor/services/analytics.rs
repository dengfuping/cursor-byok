//! Implements Cursor analytics endpoints and event handling.
use axum::{
    body::{Body, Bytes},
    extract::Extension,
    http::{header, HeaderValue, Request, Response, StatusCode},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::{BufMut, BytesMut};
use prost::Message;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::{api::cursor::proxy, Error, Result};

pub const BOOTSTRAP_STATSIG_PATH: &str = "/aiserver.v1.AnalyticsService/BootstrapStatsig";
const AGENT_RETRIES_GATE: &str = "nal_agent_retries";
const LOCAL_RULE: &str = "local_enabled";
/// Cursor rolls its server-driven agent loop out through Statsig. That loop
/// replaces `agent.v1.AgentRunRequest` with `aiserver.v1.RunInferenceClientMessage`,
/// which a local BYOK server cannot serve: the run, model routing, prompt
/// assembly and the agent loop all move to Cursor's backend. Keep the client on
/// the protocol this server implements.
const RUN_INFERENCE_CONFIG: &str = "run_inference";
const RUN_INFERENCE_WEBSOCKET_CLIENT: &str = "run_inference_websocket_client";

#[derive(Clone, PartialEq, Message)]
struct BootstrapStatsigResponse {
    #[prost(string, tag = "1")]
    config: String,
    #[prost(uint64, tag = "2")]
    generated_at_ms: u64,
}

pub async fn bootstrap_statsig(
    Extension(upstream): Extension<proxy::CursorProxy>,
    request: Request<Body>,
) -> Result<Response<Body>> {
    match proxy::forward_buffered(&upstream, request).await {
        Ok(response) if response.status.is_success() => match patch_upstream(response) {
            Ok(response) => Ok(response),
            Err(error) => {
                tracing::warn!(%error, "Cursor Statsig bootstrap was invalid; using local bootstrap");
                local_response()
            }
        },
        Ok(response) => {
            tracing::warn!(status = %response.status, "Cursor Statsig bootstrap was rejected; using local bootstrap");
            local_response()
        }
        Err(error) => {
            tracing::warn!(%error, "Cursor Statsig bootstrap was unavailable; using local bootstrap");
            local_response()
        }
    }
}

fn patch_upstream(response: proxy::BufferedResponse) -> Result<Response<Body>> {
    let (framed, payload) = unary_payload(&response.body)?;
    let mut message = BootstrapStatsigResponse::decode(payload)?;
    let mut config = serde_json::from_str::<Value>(&message.config)?;
    enable_agent_retries(&mut config)?;
    let disabled = disable_run_inference(&mut config);
    tracing::info!(
        disabled,
        inference_configs = %inference_config_names(&config),
        "patched Cursor Statsig bootstrap"
    );
    message.config = serde_json::to_string(&config)?;
    Ok(response.with_body(encode_unary(&message, framed)))
}

/// Turns every `run_inference*` gate off and forces the websocket client config
/// to stay disabled, so the client keeps using `agent.v1` frames.
fn disable_run_inference(config: &mut Value) -> usize {
    let mut disabled = 0;
    if let Some(gates) = config
        .get_mut("feature_gates")
        .and_then(Value::as_object_mut)
    {
        for (name, gate) in gates.iter_mut() {
            if !is_run_inference(name) {
                continue;
            }
            gate["value"] = Value::Bool(false);
            disabled += 1;
        }
    }
    for section in ["dynamic_configs", "layer_configs"] {
        let Some(entries) = config.get_mut(section).and_then(Value::as_object_mut) else {
            continue;
        };
        for (name, entry) in entries.iter_mut() {
            let mut hits = disable_run_inference_fields(entry);
            if is_run_inference(name) {
                if let Some(object) = entry.get_mut("value").and_then(Value::as_object_mut) {
                    object.insert("enabled".into(), Value::Bool(false));
                    hits += 1;
                }
            }
            if hits > 0 {
                tracing::info!(
                    section,
                    config = name.as_str(),
                    "disabled run inference config"
                );
                disabled += hits;
            }
        }
    }
    disabled
}

/// Forces `run_inference_websocket_client` settings to their disabled shape.
fn disable_run_inference_fields(value: &mut Value) -> usize {
    let mut disabled = 0;
    match value {
        Value::Object(object) => {
            if let Some(settings) = object.get_mut(RUN_INFERENCE_WEBSOCKET_CLIENT) {
                *settings = disabled_run_inference_websocket_client(settings.take());
                disabled += 1;
            }
            for (_key, child) in object.iter_mut() {
                disabled += disable_run_inference_fields(child);
            }
        }
        Value::Array(items) => {
            for item in items {
                disabled += disable_run_inference_fields(item);
            }
        }
        _ => {}
    }
    disabled
}

fn disabled_run_inference_websocket_client(current: Value) -> Value {
    let mut settings = current.as_object().cloned().unwrap_or_default();
    settings.insert("enabled".into(), Value::Bool(false));
    settings.insert("treatment_percent".into(), Value::from(0));
    Value::Object(settings)
}

fn is_run_inference(name: &str) -> bool {
    name.contains(RUN_INFERENCE_CONFIG)
}

/// Names of the statsig entries that mention inference, for the diagnostic log.
fn inference_config_names(config: &Value) -> String {
    let mut names = Vec::new();
    for section in ["feature_gates", "dynamic_configs", "layer_configs"] {
        let Some(entries) = config.get(section).and_then(Value::as_object) else {
            continue;
        };
        for (name, entry) in entries {
            let mentions = is_run_inference(name)
                || serde_json::to_string(entry).is_ok_and(|text| is_run_inference(&text));
            if mentions {
                names.push(format!("{section}/{name}"));
            }
        }
    }
    names.join(",")
}

fn local_response() -> Result<Response<Body>> {
    let generated_at_ms = chrono::Utc::now().timestamp_millis() as u64;
    let mut config = json!({
        "feature_gates": {},
        "dynamic_configs": {},
        "layer_configs": {},
        "user": {
            "userID": "local_ultra",
            "customIDs": { "localUserID": "local_ultra" }
        },
        "has_updates": true,
        "hash_used": "none",
        "sdkParams": {
            "stableID": "local_ultra",
            "disableDiagnosticsLogging": true
        },
        "time": generated_at_ms
    });
    enable_agent_retries(&mut config)?;
    let message = BootstrapStatsigResponse {
        config: serde_json::to_string(&config)?,
        generated_at_ms,
    };
    let body = message.encode_to_vec();
    let mut response = Response::new(Body::from(body.clone()));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/proto"),
    );
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        body.len()
            .to_string()
            .parse()
            .expect("body length is a valid header value"),
    );
    Ok(response)
}

fn enable_agent_retries(config: &mut Value) -> Result<()> {
    let gate_key = statsig_key(config, AGENT_RETRIES_GATE);
    let root = config
        .as_object_mut()
        .ok_or_else(|| Error::Protocol("Statsig bootstrap config must be an object".into()))?;
    let gates = root
        .entry("feature_gates")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| Error::Protocol("Statsig feature_gates must be an object".into()))?;
    gates.insert(gate_key.clone(), enabled_gate(&gate_key));
    Ok(())
}

fn statsig_key(config: &Value, name: &str) -> String {
    match config.get("hash_used").and_then(Value::as_str) {
        Some("djb2") => djb2(name),
        Some("sha256") => STANDARD.encode(Sha256::digest(name.as_bytes())),
        _ => name.to_owned(),
    }
}

fn djb2(value: &str) -> String {
    value
        .encode_utf16()
        .fold(0_u32, |hash, character| {
            hash.wrapping_mul(31).wrapping_add(u32::from(character))
        })
        .to_string()
}

fn enabled_gate(name: &str) -> Value {
    json!({
        "name": name,
        "value": true,
        "rule_id": LOCAL_RULE,
        "ruleID": LOCAL_RULE,
        "group_name": LOCAL_RULE,
        "groupName": LOCAL_RULE,
        "secondary_exposures": [],
        "secondaryExposures": [],
        "undelegated_secondary_exposures": [],
        "undelegatedSecondaryExposures": [],
        "is_device_based": false,
        "isDeviceBased": false,
        "id_type": "userID",
        "idType": "userID"
    })
}

fn unary_payload(body: &Bytes) -> Result<(bool, &[u8])> {
    if body.len() < 5 {
        return Ok((false, body));
    }
    let flags = body[0];
    let length = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) as usize;
    if length != body.len() - 5 {
        return Ok((false, body));
    }
    if flags != 0 {
        return Err(Error::Protocol(format!(
            "cannot patch compressed or terminal Statsig frame: flags={flags}"
        )));
    }
    Ok((true, &body[5..]))
}

fn encode_unary(message: &impl Message, framed: bool) -> Bytes {
    let payload = message.encode_to_vec();
    if !framed {
        return Bytes::from(payload);
    }
    let mut output = BytesMut::with_capacity(5 + payload.len());
    output.put_u8(0);
    output.put_u32(payload.len() as u32);
    output.extend_from_slice(&payload);
    output.freeze()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn statsig(config: Value) -> Value {
        let mut bootstrap = json!({
            "feature_gates": {},
            "dynamic_configs": {},
            "layer_configs": {},
            "hash_used": "none"
        });
        for (section, entries) in config.as_object().unwrap() {
            for (name, value) in entries.as_object().unwrap() {
                bootstrap[section][name] = json!({ "name": name, "value": value });
            }
        }
        bootstrap
    }

    #[test]
    fn run_inference_websocket_client_is_forced_off() {
        let mut config = statsig(json!({
            "dynamic_configs": {
                "agent_transport": {
                    "run_inference_websocket_client": {
                        "enabled": true,
                        "treatment_percent": 100,
                        "maxConnections": 4,
                        "fallbackOnColdPool": true
                    }
                }
            }
        }));

        assert_eq!(disable_run_inference(&mut config), 1);

        let settings = &config["dynamic_configs"]["agent_transport"]["value"]
            ["run_inference_websocket_client"];
        assert_eq!(settings["enabled"], json!(false));
        assert_eq!(settings["treatment_percent"], json!(0));
        assert_eq!(
            settings["maxConnections"],
            json!(4),
            "unrelated settings have to survive"
        );
    }

    #[test]
    fn run_inference_gates_and_named_configs_are_disabled() {
        let mut config = statsig(json!({
            "feature_gates": {
                "run_inference_rollout": true,
                "unrelated_gate": true
            },
            "dynamic_configs": {
                "run_inference_client": { "enabled": true, "maxConnections": 2 }
            }
        }));

        assert_eq!(disable_run_inference(&mut config), 2);

        assert_eq!(
            config["feature_gates"]["run_inference_rollout"]["value"],
            json!(false)
        );
        assert_eq!(
            config["feature_gates"]["unrelated_gate"]["value"],
            json!(true)
        );
        assert_eq!(
            config["dynamic_configs"]["run_inference_client"]["value"]["enabled"],
            json!(false)
        );
        assert_eq!(
            config["dynamic_configs"]["run_inference_client"]["value"]["maxConnections"],
            json!(2)
        );
    }

    #[test]
    fn inference_config_names_reports_what_was_found() {
        let config = statsig(json!({
            "dynamic_configs": {
                "agent_transport": { "run_inference_websocket_client": { "enabled": true } },
                "other": { "enabled": true }
            }
        }));

        assert_eq!(
            inference_config_names(&config),
            "dynamic_configs/agent_transport"
        );
    }
}
