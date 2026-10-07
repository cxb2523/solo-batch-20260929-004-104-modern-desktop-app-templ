//! System-tray domain. The menu/icon construction stays in `crate::tray_icon`;
//! only the invoke contract lives here.

use super::{
  CommandSpec,
  Domain,
  DuplicatePolicy,
  Layer,
  ParamSpec,
  Registry,
};
use crate::tray_icon::{TRAY_ID, create_tray_menu};

const DOMAIN: Domain = Domain::Tray;
const LAYER: Layer = Layer::Domain;

pub(in super::super) fn specs() -> &'static [CommandSpec] {
  static SPECS: &[CommandSpec] = &[CommandSpec {
    name: "tray_update_language",
    domain: DOMAIN,
    layer: LAYER,
    summary: "Rebuild the tray menu using the given locale.",
    params: &[ParamSpec::required("lang", "String")],
    returns: "()",
    acl: &[],
  }];
  SPECS
}

pub(in super::super) fn register(registry: &mut Registry) -> Result<(), super::RegistryError> {
  registry.register_many(DuplicatePolicy::Err, DOMAIN, LAYER, specs())
}

#[tauri::command]
#[allow(unused_must_use)]
pub(in crate::commands) fn tray_update_language(app: tauri::AppHandle, lang: String) {
  if let Some(tray) = app.tray_by_id(TRAY_ID) {
    tray.set_menu(create_tray_menu(&app, lang).ok());
  }
}
