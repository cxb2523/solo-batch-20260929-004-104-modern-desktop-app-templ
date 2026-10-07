// this hides the console for Windows release builds
#![cfg_attr(
  all(not(debug_assertions), target_os = "windows"),
  windows_subsystem = "windows"
)]

use serde::Serialize;
use std::sync::Mutex;
use tauri::{
  // state is used in Linux
  self,
  Emitter,
  Manager,
};
use tauri_plugin_store;
use tauri_plugin_window_state;

mod commands;
mod tray_icon;
mod utils;

use tray_icon::{TrayState, create_tray_icon};
use utils::long_running_thread;

#[derive(Clone, Serialize)]
struct SingleInstancePayload {
  args: Vec<String>,
  cwd: String,
}

#[cfg(target_os = "linux")]
pub struct DbusState(Mutex<Option<dbus::blocking::SyncConnection>>);

#[cfg(target_os = "linux")]
unsafe fn webkit_hidpi_workaround() {
  // See: https://github.com/spacedriveapp/spacedrive/issues/1512#issuecomment-1758550164
  std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
}

unsafe fn main_prelude() {
  #[cfg(target_os = "linux")]
  webkit_hidpi_workaround();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  // SAFETY: running on single thread
	unsafe { main_prelude(); }
  // main window should be invisible to allow either the setup delay or the plugin to show the window
  let mut log_builder = tauri_plugin_log::Builder::new().target(tauri_plugin_log::Target::new(
    tauri_plugin_log::TargetKind::LogDir {
      file_name: Some("logs".to_string()),
    },
  ));
  #[cfg(debug_assertions)]
  {
    log_builder = log_builder.target(tauri_plugin_log::Target::new(
      tauri_plugin_log::TargetKind::Webview,
    ));
  }

  let builder = commands::attach_invoke_handler(tauri::Builder::default());
  builder
    .plugin(log_builder.build())
    .plugin(tauri_plugin_opener::init())
    .plugin(tauri_plugin_store::Builder::new().build())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_process::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_os::init())
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_fs::init())
    // allow only one instance and propagate args and cwd to existing instance
    .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
      app
        .emit("newInstance", SingleInstancePayload { args, cwd })
        .unwrap();
    }))
    // persistent storage with filesystem
    .plugin(tauri_plugin_store::Builder::default().build())
    // save window position and size between sessions
    // if you remove this, make sure to uncomment the mainWebview?.show line in TauriProvider.tsx
    .plugin(tauri_plugin_window_state::Builder::default().build())
    // custom setup code
    .setup(|app| {
      // Assemble the command contract once; every window reads the same state.
      commands::install_managed_state(app.handle());
      let _ = create_tray_icon(app.handle());
      app.manage(Mutex::new(TrayState::NotPlaying));

      let app_handle = app.handle().clone();
      tauri::async_runtime::spawn(async move { long_running_thread(&app_handle).await });

      #[cfg(target_os = "linux")]
      app.manage(DbusState(Mutex::new(
        dbus::blocking::SyncConnection::new_session().ok(),
      )));

      // TODO: AUTOSTART
      // FOLLOW: https://v2.tauri.app/plugin/autostart/

      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}

// useful crates
// https://crates.io/crates/directories for getting common directories

// TODO: optimize permissions
// TODO: decorations false and use custom title bar
