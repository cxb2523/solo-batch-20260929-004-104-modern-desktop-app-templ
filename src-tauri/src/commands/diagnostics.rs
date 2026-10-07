//! Diagnostics domain: the read-only mirror of the assembled contract.
//!
//! The command count and the ACL verdicts the panel renders come entirely
//! from runtime reflection over [`ContractState`] and the live windows; the
//! frontend hard-codes no command names.

use serde::Serialize;
use tauri::{AppHandle, State};

use super::{
  CommandSpec,
  ContractState,
  Domain,
  DuplicatePolicy,
  Layer,
  ParamSpec,
  Registry,
};
use super::acl::{self, AclSnapshot, CommandView};

const DOMAIN: Domain = Domain::Diagnostics;
const LAYER: Layer = Layer::Domain;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in super::super) struct SnapshotView {
  pub(in super::super) policy: DuplicatePolicy,
  /// Reflective command list in fixed assembly order.
  pub(in super::super) commands: Vec<CommandView>,
  pub(in super::super) command_count: usize,
  /// Names in assembly order, used for line-by-line diffing on the panel.
  pub(in super::super) command_names: Vec<String>,
  pub(in super::super) acl: AclSnapshot,
  /// Backend-side guarantee: reflection count must match the registry count.
  pub(in super::super) registry_consistent: bool,
}

pub(in super::super) fn specs() -> &'static [CommandSpec] {
  static SPECS: &[CommandSpec] = &[CommandSpec {
    name: "diagnostics_contract_snapshot",
    domain: DOMAIN,
    layer: LAYER,
    summary: "Return the runtime command registry and ACL reflection.",
    params: &[],
    returns: "SnapshotView",
    acl: &[],
  }];
  SPECS
}

pub(in super::super) fn register(registry: &mut Registry) -> Result<(), super::RegistryError> {
  registry.register_many(DuplicatePolicy::Err, DOMAIN, LAYER, specs())
}

#[tauri::command]
pub(in crate::commands) fn contract_snapshot(
  app: AppHandle,
  contract: State<'_, ContractState>,
) -> SnapshotView {
  let acl_snapshot = acl::build(&app);
  let commands = acl::command_views(&contract.commands, &acl_snapshot);
  let command_count = commands.len();
  let command_names: Vec<String> = commands.iter().map(|command| command.name.clone()).collect();
  SnapshotView {
    policy: contract.policy,
    commands,
    command_count,
    command_names,
    registry_consistent: command_count == contract.commands.len(),
    acl: acl_snapshot,
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn snapshot_command_is_last_in_assembly() {
    let names = super::super::tests::assembled_names();
    assert_eq!(
      names.last().map(String::as_str),
      Some("diagnostics_contract_snapshot")
    );
  }

  #[test]
  fn every_command_exposes_contract_metadata() {
    let contract = super::super::tests::assembled_contract();
    for command in &contract.commands {
      assert!(!command.name.is_empty());
      assert!(!command.summary.is_empty(), "{} lacks a summary", command.name);
      assert!(!command.returns.is_empty());
      for param in command.params {
        assert!(!param.name.is_empty());
      }
    }
  }
}
