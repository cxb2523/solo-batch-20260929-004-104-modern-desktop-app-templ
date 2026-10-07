//! Backend contract layer.
//!
//! Every domain below is self-describing: it exports exactly one `register`
//! entry point that appends its invoke commands to the running [`Registry`].
//! `lib.rs` assembles the domains in a fixed order, attaches the matching
//! Tauri invoke handler and installs the managed state. The frontend never
//! declares commands; it only reads the assembled contract through the
//! `diagnostics` domain.

mod acl;
mod crosscutting;
mod diagnostics;
mod file;
mod tray;

use std::collections::HashSet;

use serde::Serialize;
use tauri::{Builder, Manager, Runtime, Wry};

/// How a domain reacts when one of its commands collides with an existing
/// name. Each domain picks its own policy, but every domain inside one process
/// must agree: the first domain fixes the process-wide policy and any domain
/// declaring a different policy is rejected at assembly time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DuplicatePolicy {
  /// Return a [`RegistryError`] from the registration entry.
  Err,
  /// Abort registration immediately with a panic.
  Panic,
}

/// Functional area a command belongs to. Cross-domain commands live in
/// [`Domain::Crosscutting`] and are additionally tagged with
/// [`Layer::Crosscutting`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Domain {
  File,
  Tray,
  Crosscutting,
  Diagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Layer {
  /// Command owned by a single bounded domain.
  Domain,
  /// Command that spans domains (window lifecycle, theme, tray shell).
  Crosscutting,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ParamSpec {
  name: &'static str,
  required: bool,
  #[serde(rename = "type")]
  ty: &'static str,
  #[serde(skip_serializing_if = "str::is_empty")]
  default: &'static str,
}

impl ParamSpec {
  pub(crate) const fn required(name: &'static str, ty: &'static str) -> Self {
    ParamSpec {
      name,
      required: true,
      ty,
      default: "",
    }
  }

  /// Optional parameters may only ever be appended after the required ones;
  /// removing or retyping an existing parameter breaks the append-only rule.
  pub(crate) const fn optional(
    name: &'static str,
    ty: &'static str,
    default: &'static str,
  ) -> Self {
    ParamSpec {
      name,
      required: false,
      ty,
      default,
    }
  }

  fn required_flag(&self) -> bool {
    self.required
  }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct CommandSpec {
  /// Name the frontend passes to `invoke`; globally unique per process.
  name: &'static str,
  domain: Domain,
  layer: Layer,
  summary: &'static str,
  /// Ordered parameter contract; required parameters always come first so
  /// that the contract can only evolve by appending optional fields.
  params: &'static [ParamSpec],
  returns: &'static str,
  /// ACL permission identifiers the command needs. Application commands are
  /// not gated by the ACL, so this list is empty; non-empty entries describe
  /// plugin-backed commands and drive the runtime allow/deny verdict.
  acl: &'static [&'static str],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RegistryErrorKind {
  /// A command with the same invoke name was already registered.
  Duplicate,
  /// The domain declared a different duplicate policy than earlier domains.
  PolicyMismatch,
}

#[derive(Debug)]
pub(crate) struct RegistryError {
  kind: RegistryErrorKind,
  /// Invoke name involved (empty for process-wide policy failures).
  command: String,
}

impl RegistryError {
  fn duplicate(command: &str) -> Self {
    RegistryError {
      kind: RegistryErrorKind::Duplicate,
      command: command.to_string(),
    }
  }

  fn policy_mismatch(command: &str) -> Self {
    RegistryError {
      kind: RegistryErrorKind::PolicyMismatch,
      command: command.to_string(),
    }
  }

  pub(crate) fn kind(&self) -> &RegistryErrorKind {
    &self.kind
  }

  pub(crate) fn command(&self) -> &str {
    &self.command
  }
}

impl std::fmt::Display for RegistryError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self.kind {
      RegistryErrorKind::Duplicate => {
        write!(f, "duplicate command registration: `{}`", self.command)
      }
      RegistryErrorKind::PolicyMismatch => write!(
        f,
        "duplicate registration policy mismatch around `{}`: all domains must agree",
        self.command
      ),
    }
  }
}

impl std::error::Error for RegistryError {}

/// Runtime self-registry. Domains push their specs into it while the process
/// starts; once [`Registry::freeze`] runs the contract is immutable and is
/// shared read-only with every window.
pub(crate) struct Registry {
  policy: Option<DuplicatePolicy>,
  commands: Vec<CommandSpec>,
  names: HashSet<&'static str>,
}

impl Registry {
  fn new() -> Self {
    Registry {
      policy: None,
      commands: Vec::new(),
      names: HashSet::new(),
    }
  }

  pub(crate) fn register_many(
    &mut self,
    policy: DuplicatePolicy,
    domain: Domain,
    layer: Layer,
    specs: &'static [CommandSpec],
  ) -> Result<(), RegistryError> {
    let anchor = specs.first().map(|spec| spec.name).unwrap_or("");
    match self.policy {
      None => self.policy = Some(policy),
      Some(existing) if existing != policy => {
        return Err(RegistryError::policy_mismatch(anchor));
      }
      Some(_) => {}
    }

    for spec in specs {
      assert_eq!(domain, spec.domain, "spec domain must match its registrar");
      assert_eq!(layer, spec.layer, "spec layer must match its registrar");
      if !self.names.insert(spec.name) {
        match policy {
          DuplicatePolicy::Err => return Err(RegistryError::duplicate(spec.name)),
          DuplicatePolicy::Panic => panic!(
            "duplicate command registration: `{}` (domain {:?})",
            spec.name,
            spec.domain
          ),
        }
      }
      self.commands.push(spec.clone());
    }
    Ok(())
  }

  fn freeze(self) -> (DuplicatePolicy, Vec<CommandSpec>) {
    (self.policy.unwrap_or(DuplicatePolicy::Err), self.commands)
  }
}

/// Frozen contract handed to the diagnostics domain.
pub(crate) struct ContractState {
  policy: DuplicatePolicy,
  commands: Vec<CommandSpec>,
}

impl ContractState {
  fn new(policy: DuplicatePolicy, commands: Vec<CommandSpec>) -> Self {
    ContractState { policy, commands }
  }
}

/// Assemble every domain in the fixed startup order. The order is the order
/// the diagnostics panel displays and the order Tauri receives handlers, so
/// reordering here is a contract change.
fn assemble() -> Result<ContractState, RegistryError> {
  let mut registry = Registry::new();
  file::register(&mut registry)?;
  tray::register(&mut registry)?;
  // cross-domain commands are isolated in their own layer on purpose.
  crosscutting::register(&mut registry)?;
  diagnostics::register(&mut registry)?;
  let (policy, commands) = registry.freeze();
  Ok(ContractState::new(policy, commands))
}

/// Attach the invoke handlers in the exact assembly order. The handler list
/// lives inside the contract layer so a domain still exposes `register` as
/// its single public surface.
pub(crate) fn attach_invoke_handler(builder: Builder<Wry>) -> Builder<Wry> {
  builder.invoke_handler(tauri::generate_handler![
    file::file_process,
    tray::tray_update_language,
    crosscutting::theme_get,
    crosscutting::theme_set,
    crosscutting::window_open,
    diagnostics::contract_snapshot,
  ])
}

/// Build the contract once and install the managed state every window shares.
pub(crate) fn install_managed_state<R: Runtime>(app: &tauri::AppHandle<R>) {
  let contract = assemble().expect("command contract assembly failed");
  app.manage(contract);
  app.manage(crosscutting::ThemeState::load(app));
  app.manage(crosscutting::WindowCounter::default());
}

/// Error payload returned by fallible application commands. Codes are stable
/// strings so old error-code semantics never shift.
#[derive(Debug, Serialize)]
pub(crate) struct CommandError {
  code: &'static str,
  message: String,
}

impl CommandError {
  pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
    CommandError {
      code,
      message: message.into(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  pub(crate) fn assembled_contract() -> ContractState {
    assemble().expect("assembly must succeed")
  }

  pub(crate) fn assembled_names() -> Vec<String> {
    assembled_contract()
      .commands
      .iter()
      .map(|spec| spec.name.to_string())
      .collect()
  }

  #[test]
  fn assemble_has_fixed_order_and_count() {
    let contract = assemble().expect("assembly must succeed");
    let names: Vec<String> = contract
      .commands
      .iter()
      .map(|spec| spec.name.to_string())
      .collect();
    assert!(
      names.iter().map(String::as_str).eq([
        "file_process",
        "tray_update_language",
        "theme_get",
        "theme_set",
        "window_open",
        "diagnostics_contract_snapshot",
      ]),
      "assembly order is a frozen contract: {names:?}"
    );
    assert_eq!(contract.commands.len(), 6);
    assert_eq!(contract.policy, DuplicatePolicy::Err);
  }

  #[test]
  fn duplicate_registration_is_rejected() {
    // Registering the same domain twice must fail on the spot.
    let mut registry = Registry::new();
    file::register(&mut registry).expect("first registration succeeds");
    let error = file::register(&mut registry).expect_err("duplicate must be rejected");
    assert_eq!(error.kind(), &RegistryErrorKind::Duplicate);
    assert_eq!(error.command(), "file_process");
    println!("DUPLICATE_REGISTRATION_REJECTED: {error}");
  }

  #[test]
  #[should_panic(expected = "duplicate command registration")]
  fn panic_policy_domain_fails_on_the_spot() {
    // A domain that declared the panic policy aborts the process instead.
    const PANIC_SPECS: &[CommandSpec] = &[CommandSpec {
      name: "test_panic_cmd",
      domain: Domain::Tray,
      layer: Layer::Domain,
      summary: "",
      params: &[],
      returns: "()",
      acl: &[],
    }];
    let mut registry = Registry::new();
    registry
      .register_many(
        DuplicatePolicy::Panic,
        Domain::Tray,
        Layer::Domain,
        PANIC_SPECS,
      )
      .unwrap();
    registry
      .register_many(
        DuplicatePolicy::Panic,
        Domain::Tray,
        Layer::Domain,
        PANIC_SPECS,
      )
      .unwrap();
  }

  #[test]
  fn duplicate_policy_must_be_uniform_across_domains() {
    let mut registry = Registry::new();
    file::register(&mut registry).unwrap();
    // Another domain claiming the panic policy contradicts the running process.
    let error = registry
      .register_many(
        DuplicatePolicy::Panic,
        Domain::Tray,
        Layer::Domain,
        &[],
      )
      .expect_err("policy mismatch must be rejected");
    assert_eq!(error.kind(), &RegistryErrorKind::PolicyMismatch);
  }

  #[test]
  fn optional_parameters_are_only_appended() {
    let contract = assemble().unwrap();
    for command in &contract.commands {
      let params = &command.params;
      if let Some(first_optional) = params.iter().position(|param| !param.required_flag()) {
        assert!(
          params[first_optional..].iter().all(|param| !param.required_flag()),
          "command `{}` adds a required parameter after an optional one",
          command.name
        );
      }
    }
  }

  #[test]
  fn contract_is_identical_from_concurrent_threads() {
    let handles: Vec<_> = (0..4)
      .map(|_| {
        std::thread::spawn(|| {
          let contract = assemble().expect("assembly always succeeds");
          serde_json::to_value(&contract.commands).unwrap()
        })
      })
      .collect();
    let snapshots: Vec<_> = handles
      .into_iter()
      .map(|handle| handle.join().unwrap())
      .collect();
    assert!(snapshots.windows(2).all(|pair| pair[0] == pair[1]));
  }
}
