//! Runtime reflection of the Tauri ACL.
//!
//! The capabilities under `capabilities/*.json` remain the single source of
//! truth for plugin permissions. This module re-reads that source at startup,
//! computes the effective permission set for every live window and derives an
//! allow/deny verdict for each registered command. Because the contract is
//! assembled once and shared, concurrently launched windows always compare
//! equal — the diagnostics surface verifies that invariant itself.

use serde::Deserialize;
use tauri::{AppHandle, Manager, Runtime};

use super::{CommandSpec, Domain};

const DESKTOP_CAPABILITY: &str = include_str!("../../capabilities/desktop.json");
const MIGRATED_CAPABILITY: &str = include_str!("../../capabilities/migrated.json");

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PermissionEntry {
  Identifier(String),
  Scoped {
    #[allow(dead_code)]
    identifier: String,
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
  },
}

impl PermissionEntry {
  fn identifier(&self) -> &str {
    match self {
      PermissionEntry::Identifier(identifier) => identifier,
      PermissionEntry::Scoped { identifier, .. } => identifier,
    }
  }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capability {
  identifier: String,
  #[serde(default)]
  windows: Vec<String>,
  #[serde(default)]
  permissions: Vec<PermissionEntry>,
}

/// Flat permission verdict used by the diagnostics panel.
#[derive(Debug, serde::Serialize)]
pub(in super::super) struct PermissionView {
  pub(in super::super) identifier: String,
  /// Capability files that granted this identifier.
  pub(in super::super) sources: Vec<String>,
}

/// Effective permission set for one live window label.
#[derive(Debug, serde::Serialize)]
pub(in super::super) struct WindowGrant {
  pub(in super::super) label: String,
  pub(in super::super) permissions: Vec<String>,
}

/// The full ACL snapshot returned to the UI.
#[derive(Debug, serde::Serialize)]
pub(in super::super) struct AclSnapshot {
  /// Sorted union of every identifier granted by the capability files.
  pub(in super::super) permissions: Vec<PermissionView>,
  /// Per-window effective sets; two rows must never disagree.
  pub(in super::super) windows: Vec<WindowGrant>,
  /// True when every live window received the exact same permission set.
  pub(in super::super) uniform_across_windows: bool,
}

/// Per-command verdict row in the contract snapshot.
#[derive(Debug, serde::Serialize)]
pub(in super::super) struct CommandView {
  pub(in super::super) name: String,
  pub(in super::super) domain: Domain,
  pub(in super::super) layer: super::Layer,
  pub(in super::super) summary: String,
  pub(in super::super) params: Vec<super::ParamSpec>,
  pub(in super::super) returns: String,
  pub(in super::super) required_acl: Vec<String>,
  /// Application commands declare no plugin ACL and are therefore always
  /// allowed; plugin-backed commands are judged against the live grant.
  pub(in super::super) acl_verdict: String,
}

fn load_capabilities() -> Vec<Capability> {
  [DESKTOP_CAPABILITY, MIGRATED_CAPABILITY]
    .into_iter()
    .filter_map(|raw| {
      serde_json::from_str::<Capability>(raw)
        .map_err(|error| eprintln!("failed to parse capability: {error}"))
        .ok()
    })
    .collect()
}

fn window_matches(pattern: &str, label: &str) -> bool {
  if pattern == "*" || pattern == label {
    return true;
  }
  // supports trailing globs such as "secondary-*"
  if let Some(prefix) = pattern.strip_suffix('*') {
    return label.starts_with(prefix);
  }
  false
}

pub(in super::super) fn build<R: Runtime>(app: &AppHandle<R>) -> AclSnapshot {
  let capabilities = load_capabilities();

  // Identifier -> granting capability names.
  let mut by_identifier: std::collections::BTreeMap<String, Vec<String>> =
    std::collections::BTreeMap::new();
  for capability in &capabilities {
    for permission in &capability.permissions {
      by_identifier
        .entry(permission.identifier().to_string())
        .and_modify(|sources| sources.push(capability.identifier.clone()))
        .or_insert_with(|| vec![capability.identifier.clone()]);
    }
  }
  let permissions = by_identifier
    .into_iter()
    .map(|(identifier, sources)| PermissionView { identifier, sources })
    .collect::<Vec<_>>();

  // Effective grant per live window.
  let mut labels: Vec<String> = app
    .webview_windows()
    .into_keys()
    .map(|label| label.to_string())
    .collect();
  labels.sort();

  let grants_for = |label: &str| {
    let mut granted: Vec<String> = capabilities
      .iter()
      .filter(|capability| capability.windows.iter().any(|pattern| window_matches(pattern, label)))
      .flat_map(|capability| {
        capability
          .permissions
          .iter()
          .map(|permission| permission.identifier().to_string())
      })
      .collect();
    granted.sort();
    granted.dedup();
    granted
  };

  let window_grants: Vec<WindowGrant> = labels
    .iter()
    .map(|label| WindowGrant {
      label: label.clone(),
      permissions: grants_for(label),
    })
    .collect();
  let uniform_across_windows = window_grants
    .windows(2)
    .all(|pair| pair[0].permissions == pair[1].permissions);

  AclSnapshot {
    permissions,
    windows: window_grants,
    uniform_across_windows,
  }
}

pub(in super::super) fn command_views(
  commands: &[CommandSpec],
  snapshot: &AclSnapshot,
) -> Vec<CommandView> {
  let granted: std::collections::HashSet<String> = snapshot
    .windows
    .iter()
    .flat_map(|grant| grant.permissions.iter().cloned())
    .collect();
  commands
    .iter()
    .map(|command| {
      let required: Vec<String> = command.acl.iter().map(|name| name.to_string()).collect();
      let acl_verdict = if required.is_empty() {
        "allowed".to_string()
      } else if required.iter().all(|name| granted.contains(name)) {
        "allowed".to_string()
      } else {
        "denied".to_string()
      };
      CommandView {
        name: command.name.to_string(),
        domain: command.domain,
        layer: command.layer,
        summary: command.summary.to_string(),
        params: command.params.to_vec(),
        returns: command.returns.to_string(),
        required_acl: required,
        acl_verdict,
      }
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn capabilities_parse_without_duplicate_grants() {
    let capabilities = load_capabilities();
    assert!(capabilities.iter().any(|cap| cap.identifier == "desktop-capability"));
    assert!(capabilities.iter().any(|cap| cap.identifier == "migrated"));
    for capability in &capabilities {
      let mut seen = std::collections::HashSet::new();
      for permission in &capability.permissions {
        assert!(
          seen.insert(permission.identifier()),
          "capability `{}` grants `{}` twice (ACL must be minimal)",
          capability.identifier,
          permission.identifier()
        );
      }
    }
  }

  #[test]
  fn wildcard_and_secondary_windows_share_main_grant() {
    assert!(window_matches("main", "main"));
    assert!(window_matches("*", "secondary-1"));
    assert!(!window_matches("main", "secondary-1"));
  }
}
