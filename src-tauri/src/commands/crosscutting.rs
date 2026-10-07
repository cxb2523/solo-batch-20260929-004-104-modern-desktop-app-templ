//! Crosscutting commands: concerns that span multiple domains
//! (theme/appearance and window lifecycle) and therefore cannot live inside
//! any single domain module.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use super::{
  CommandError,
  CommandSpec,
  Domain,
  DuplicatePolicy,
  Layer,
  ParamSpec,
  Registry,
};

const DOMAIN: Domain = Domain::Crosscutting;
const LAYER: Layer = Layer::Crosscutting;

/// Emitted to every window after the backend theme changes. The frontend is
/// purely a listener: reopening a window re-reads [`ThemeState`] instead of
/// trusting per-window UI state.
pub(crate) const THEME_CHANGED_EVENT: &str = "app://theme-changed";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ThemeMode {
  Light,
  Dark,
  Auto,
}

impl ThemeMode {
  fn as_str(self) -> &'static str {
    match self {
      ThemeMode::Light => "light",
      ThemeMode::Dark => "dark",
      ThemeMode::Auto => "auto",
    }
  }

  fn parse(raw: &str) -> Option<Self> {
    match raw {
      "light" => Some(ThemeMode::Light),
      "dark" => Some(ThemeMode::Dark),
      "auto" => Some(ThemeMode::Auto),
      _ => None,
    }
  }
}

#[derive(Debug, Serialize, Deserialize)]
struct ThemeFile {
  mode: String,
}

#[derive(Clone, Debug, Serialize)]
struct ThemePayload {
  mode: &'static str,
}

pub(crate) struct ThemeState {
  mode: Mutex<ThemeMode>,
  path: PathBuf,
}

impl ThemeState {
  /// Load once per process; every window shares the resulting managed state,
  /// so concurrent window launches always observe the same appearance.
  pub(crate) fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
    let dir = app
      .path()
      .app_config_dir()
      .or_else(|_| app.path().app_local_data_dir())
      .unwrap_or_else(|_| std::env::temp_dir());
    let path = dir.join("theme.json");
    let mode = fs::read_to_string(&path)
      .ok()
      .and_then(|raw| serde_json::from_str::<ThemeFile>(&raw).ok())
      .and_then(|file| ThemeMode::parse(&file.mode))
      .unwrap_or(ThemeMode::Auto);
    ThemeState {
      mode: Mutex::new(mode),
      path,
    }
  }
}

#[derive(Default)]
pub(crate) struct WindowCounter(Mutex<u32>);

pub(in super::super) fn specs() -> &'static [CommandSpec] {
  static SPECS: &[CommandSpec] = &[
    CommandSpec {
      name: "theme_get",
      domain: DOMAIN,
      layer: LAYER,
      summary: "Read the process-wide appearance mode.",
      params: &[],
      returns: "String (\"light\" | \"dark\" | \"auto\")",
      acl: &[],
    },
    CommandSpec {
      name: "theme_set",
      domain: DOMAIN,
      layer: LAYER,
      summary: "Set the appearance mode, persist it and notify all windows.",
      params: &[
        ParamSpec::required("mode", "String"),
        ParamSpec::optional("persist", "Option<bool>", "Some(true)"),
      ],
      returns: "()",
      acl: &[],
    },
    CommandSpec {
      name: "window_open",
      domain: DOMAIN,
      layer: LAYER,
      summary: "Open another main-capability window and return its label.",
      params: &[ParamSpec::optional("label", "Option<String>", "None")],
      returns: "String (window label)",
      acl: &[],
    },
  ];
  SPECS
}

pub(in super::super) fn register(registry: &mut Registry) -> Result<(), super::RegistryError> {
  registry.register_many(DuplicatePolicy::Err, DOMAIN, LAYER, specs())
}

#[tauri::command]
pub(in crate::commands) fn theme_get(theme: State<'_, ThemeState>) -> String {
  theme.mode.lock().unwrap().as_str().to_string()
}

#[tauri::command]
pub(in crate::commands) fn theme_set(
  app: AppHandle,
  theme: State<'_, ThemeState>,
  mode: String,
  persist: Option<bool>,
) -> Result<(), CommandError> {
  let next = ThemeMode::parse(&mode)
    .ok_or_else(|| CommandError::new("THEME_INVALID_MODE", format!("unknown mode `{mode}`")))?;
  {
    let mut current = theme.mode.lock().unwrap();
    *current = next;
  }

  if persist.unwrap_or(true) {
    let parent = theme.path.parent().map(|path| path.to_path_buf());
    if let Some(parent) = parent {
      fs::create_dir_all(&parent).map_err(|error| {
        CommandError::new("THEME_PERSIST_FAILED", error.to_string())
      })?;
    }
    let serialized = serde_json::to_string_pretty(&ThemeFile {
      mode: next.as_str().to_string(),
    })
      .map_err(|error| CommandError::new("THEME_PERSIST_FAILED", error.to_string()))?;
    fs::write(&theme.path, serialized)
      .map_err(|error| CommandError::new("THEME_PERSIST_FAILED", error.to_string()))?;
  }

  app
    .emit(
      THEME_CHANGED_EVENT,
      ThemePayload {
        mode: next.as_str(),
      },
    )
    .map_err(|error| CommandError::new("THEME_EVENT_FAILED", error.to_string()))
}

#[tauri::command]
pub(in crate::commands) fn window_open(
  app: AppHandle,
  counter: State<'_, WindowCounter>,
  label: Option<String>,
) -> Result<String, CommandError> {
  use tauri::WebviewUrl;
  use tauri::webview::WebviewWindowBuilder;

  let label = match label {
    Some(raw) => {
      let raw = raw.trim().to_string();
      if raw.is_empty() {
        return Err(CommandError::new("WINDOW_INVALID_LABEL", "label is empty"));
      }
      if app.get_webview_window(&raw).is_some() {
        return Err(CommandError::new(
          "WINDOW_LABEL_CONFLICT",
          format!("window `{raw}` already exists"),
        ));
      }
      raw
    }
    None => {
      let mut next = counter.0.lock().unwrap();
      let mut candidate;
      loop {
        *next += 1;
        candidate = format!("secondary-{next}");
        if app.get_webview_window(&candidate).is_none() {
          break candidate;
        }
      }
    }
  };

  // Capabilities target every window, so the new window receives exactly the
  // same permission set as `main`.
  WebviewWindowBuilder::new(
    &app,
    &label,
    WebviewUrl::App("index.html?view=diagnostics".into()),
  )
  .title("Diagnostics")
  .inner_size(1000.0, 700.0)
  .min_inner_size(700.0, 500.0)
  .resizable(true)
  .visible(true)
  .build()
  .map_err(|error| CommandError::new("WINDOW_OPEN_FAILED", error.to_string()))?;

  Ok(label)
}
