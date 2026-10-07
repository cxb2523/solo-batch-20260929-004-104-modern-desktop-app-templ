//! File-processing domain.
//!
//! Single registration entry: [`register`]. The command functions are only
//! visible inside the contract layer; the handler list lives in `mod.rs`.

use super::{
  CommandSpec,
  Domain,
  DuplicatePolicy,
  Layer,
  ParamSpec,
  Registry,
};

const DOMAIN: Domain = Domain::File;
const LAYER: Layer = Layer::Domain;

/// Static contract for every command this domain owns.
pub(in super::super) fn specs() -> &'static [CommandSpec] {
  // `note` is the first evolution of the original command: an optional field
  // appended after the unchanged, still-required `filepath`.
  static SPECS: &[CommandSpec] = &[CommandSpec {
    name: "file_process",
    domain: DOMAIN,
    layer: LAYER,
    summary: "Echo a processed message for the given file path.",
    params: &[
      ParamSpec::required("filepath", "String"),
      ParamSpec::optional("note", "Option<String>", "None"),
    ],
    returns: "String",
    acl: &[],
  }];
  SPECS
}

/// Sole domain entry point.
pub(in super::super) fn register(registry: &mut Registry) -> Result<(), super::RegistryError> {
  registry.register_many(DuplicatePolicy::Err, DOMAIN, LAYER, specs())
}

#[tauri::command]
pub(in crate::commands) fn file_process(filepath: String, note: Option<String>) -> String {
  // Legacy semantics preserved: same stdout line, same response string.
  println!("Processing file: {}", filepath);
  match note {
    Some(note) if !note.is_empty() => format!("Hello from Rust! ({note})"),
    _ => "Hello from Rust!".to_string(),
  }
}
