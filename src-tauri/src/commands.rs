use crate::cancel::OperationRegistry;
use crate::config::ConfigStore;
use crate::gateway::{GatewayError, KiloGatewayClient};
use crate::models::*;
use crate::tools::{ToolInvocation, ToolOutput, Workspace, canonical_dir, display_path, parse_arguments};
use std::sync::Arc;
use tauri::ipc::Channel;
use tauri::{State, command};

type Gateway<'a> = State<'a, Arc<KiloGatewayClient>>;

/// Get all available models from the Kilo Gateway (public, no key required).
#[command]
pub async fn get_models(gateway: Gateway<'_>) -> Result<ModelsResponse, String> {
    gateway.get_models().await.map_err(|e| e.to_string())
}

/// Get only KiloCode Free models.
#[command]
pub async fn get_available_models(gateway: Gateway<'_>) -> Result<Vec<ModelInfo>, String> {
    gateway.get_free_models().await.map_err(|e| e.to_string())
}

/// Get Kilo Gateway status.
#[command]
pub async fn get_gateway_status(gateway: Gateway<'_>) -> Result<GatewayStatus, String> {
    gateway.get_gateway_status().await.map_err(|e| e.to_string())
}

/// Non-streaming chat completion.
#[command]
pub async fn chat_completion(
    gateway: Gateway<'_>,
    request: ChatCompletionRequest,
) -> Result<ChatCompletionResponse, String> {
    gateway.chat_completion(request).await.map_err(|e| e.to_string())
}

/// Streamed chat completion. Chunks and exactly one terminal event
/// (`done` / `cancelled` / `error`) are delivered through `on_event`.
#[command]
pub async fn chat_completion_stream(
    gateway: Gateway<'_>,
    operations: State<'_, Arc<OperationRegistry>>,
    request_id: String,
    request: ChatCompletionRequest,
    on_event: Channel<StreamEvent>,
) -> Result<(), String> {
    let token = operations.register(&request_id);
    let result = gateway
        .chat_completion_stream(request, token, |event| {
            let _ = on_event.send(event);
        })
        .await;
    operations.finish(&request_id);

    let terminal = match result {
        Ok(false) => StreamEvent::Done,
        Ok(true) => StreamEvent::Cancelled,
        Err(e) => StreamEvent::Error(e.to_string()),
    };
    let _ = on_event.send(terminal);
    Ok(())
}

/// Cancel an in-flight stream or tool execution by id.
#[command]
pub fn cancel_operation(operations: State<'_, Arc<OperationRegistry>>, id: String) -> bool {
    operations.cancel(&id)
}

/// Validate and store an API key. Validation uses a single-token request to a
/// free model, so an invalid token yields 401.
#[command]
pub async fn set_api_key(gateway: Gateway<'_>, api_key: String) -> Result<ApiKeyStatus, String> {
    let api_key = api_key.trim().to_string();
    let status = gateway.validate_api_key(&api_key).await.map_err(|e| e.to_string())?;
    if status.valid {
        gateway.set_api_key(api_key);
    }
    Ok(status)
}

/// Re-validate the stored key. Returns null when no key is stored. A network
/// failure keeps the key so the user is not logged out while offline.
#[command]
pub async fn get_api_key_status(gateway: Gateway<'_>) -> Result<Option<ApiKeyStatus>, String> {
    let Some(key) = gateway.get_api_key() else {
        return Ok(None);
    };

    match gateway.validate_api_key(&key).await {
        Ok(status) => Ok(Some(status)),
        Err(GatewayError::Authentication(_)) => {
            gateway.clear_api_key();
            Ok(None)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Masked preview of the stored key, e.g. `sk-a…9f2c`.
#[command]
pub fn get_api_key_preview(gateway: Gateway<'_>) -> Option<String> {
    gateway.get_api_key().map(|key| {
        let chars: Vec<char> = key.chars().collect();
        if chars.len() <= 10 {
            "•".repeat(chars.len())
        } else {
            let head: String = chars[..6].iter().collect();
            let tail: String = chars[chars.len() - 4..].iter().collect();
            format!("{head}…{tail}")
        }
    })
}

#[command]
pub fn has_api_key(gateway: Gateway<'_>) -> bool {
    gateway.has_api_key()
}

#[command]
pub fn clear_api_key(gateway: Gateway<'_>) {
    gateway.clear_api_key();
}

#[command]
pub fn set_default_model(gateway: Gateway<'_>, model: String) {
    gateway.set_default_model(model);
}

#[command]
pub fn get_default_model(gateway: Gateway<'_>) -> String {
    gateway.get_default_model()
}

/// Workspace root as a display path, if one is set and still exists.
#[command]
pub fn get_workspace(config: State<'_, Arc<ConfigStore>>) -> Option<String> {
    config
        .get()
        .workspace
        .filter(|p| std::path::Path::new(p).is_dir())
}

#[command]
pub fn set_workspace(config: State<'_, Arc<ConfigStore>>, path: Option<String>) -> Result<Option<String>, String> {
    let display = match path {
        Some(p) => Some(display_path(&canonical_dir(&p)?)),
        None => None,
    };
    config.update(|c| c.workspace = display.clone());
    Ok(display)
}

/// Execute an agent tool inside the current workspace. Tool failures are
/// returned as `ok: false` so the model can see and recover from them.
///
/// A few tools need no workspace (`update_todos`, `plan`) — they read and write
/// nothing on disk, so they stay available even with no folder open.
#[command]
pub async fn execute_tool(
    config: State<'_, Arc<ConfigStore>>,
    operations: State<'_, Arc<OperationRegistry>>,
    call_id: String,
    invocation: ToolInvocation,
) -> Result<ToolOutput, String> {
    let args = match parse_arguments(&invocation.arguments) {
        Ok(args) => args,
        Err(e) => {
            return Ok(ToolOutput { ok: false, output: e, changed_path: None });
        }
    };

    if Workspace::is_workspace_free(&invocation.name) {
        let token = operations.register(&call_id);
        let output = match invocation.name.as_str() {
            "update_todos" => Workspace::update_todos(&args),
            "plan" => Workspace::plan(&args),
            other => Err(format!("Unknown tool `{other}`")),
        };
        operations.finish(&call_id);
        drop(token);
        return Ok(output.unwrap_or_else(|e| ToolOutput {
            ok: false,
            output: e,
            changed_path: None,
        }));
    }

    let Some(root) = config.get().workspace else {
        return Ok(ToolOutput {
            ok: false,
            output: "No workspace folder is open. Ask the user to open a workspace folder first.".into(),
            changed_path: None,
        });
    };
    let root = canonical_dir(&root)?;

    let token = operations.register(&call_id);
    let output = Workspace::new(root).execute(&invocation.name, &args, token).await;
    operations.finish(&call_id);
    Ok(output)
}

/// Absolute path for a workspace-relative path (used to reveal files).
#[command]
pub fn resolve_workspace_path(config: State<'_, Arc<ConfigStore>>, path: String) -> Result<String, String> {
    let root = config.get().workspace.ok_or("No workspace folder is open")?;
    let resolved = Workspace::new(canonical_dir(&root)?).resolve(&path)?;
    Ok(display_path(&resolved))
}
