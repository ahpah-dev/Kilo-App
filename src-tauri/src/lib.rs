mod cancel;
mod commands;
mod config;
mod gateway;
mod models;
mod tools;

use cancel::OperationRegistry;
use config::ConfigStore;
use gateway::KiloGatewayClient;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .plugin(
      tauri_plugin_log::Builder::default()
        .level(if cfg!(debug_assertions) {
          log::LevelFilter::Debug
        } else {
          log::LevelFilter::Info
        })
        .build(),
    )
    .plugin(tauri_plugin_http::init())
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_dialog::init())
    .setup(|app| {
      let config = match app.path().app_config_dir() {
        Ok(dir) => ConfigStore::load(dir.join("config.json")),
        Err(e) => {
          log::warn!("no app config dir, settings will not persist: {e}");
          ConfigStore::in_memory()
        }
      };
      let config = Arc::new(config);

      app.manage(Arc::new(KiloGatewayClient::new(config.clone())));
      app.manage(config);
      app.manage(Arc::new(OperationRegistry::default()));
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::get_models,
      commands::chat_completion,
      commands::chat_completion_stream,
      commands::cancel_operation,
      commands::get_gateway_status,
      commands::get_available_models,
      commands::get_gateway_url,
      commands::set_gateway_url,
      commands::set_api_key,
      commands::get_api_key_status,
      commands::get_api_key_preview,
      commands::has_api_key,
      commands::clear_api_key,
      commands::set_default_model,
      commands::get_default_model,
      commands::get_workspace,
      commands::set_workspace,
      commands::execute_tool,
      commands::resolve_workspace_path,
      commands::read_text_file,
    ])
    .run(tauri::generate_context!())
    .expect("error while building tauri application");
}
