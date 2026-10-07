// Typed read-side of the backend contract layer.
//
// Nothing here is hand-maintained command data: every type describes what the
// `diagnostics_contract_snapshot` command reflects from the running process.
// The UI is only a reader.
import { invoke, isTauri } from '@tauri-apps/api/core';

export type DuplicatePolicy = 'err' | 'panic';
export type CommandDomain = 'file' | 'tray' | 'crosscutting' | 'diagnostics';
export type CommandLayer = 'domain' | 'crosscutting';
export type AclVerdict = 'allowed' | 'denied';

export interface ParamSpec {
	name: string;
	required: boolean;
	type: string;
	default?: string;
}

export interface CommandView {
	name: string;
	domain: CommandDomain;
	layer: CommandLayer;
	summary: string;
	params: ParamSpec[];
	returns: string;
	requiredAcl: string[];
	aclVerdict: AclVerdict;
}

export interface PermissionView {
	identifier: string;
	sources: string[];
}

export interface WindowGrant {
	label: string;
	permissions: string[];
}

export interface AclSnapshot {
	permissions: PermissionView[];
	windows: WindowGrant[];
	uniformAcrossWindows: boolean;
}

export interface ContractSnapshot {
	policy: DuplicatePolicy;
	commands: CommandView[];
	commandCount: number;
	commandNames: string[];
	acl: AclSnapshot;
	registryConsistent: boolean;
}

export const THEME_CHANGED_EVENT = 'app://theme-changed';
export type ThemeMode = 'light' | 'dark' | 'auto';

export const contractApi = {
	snapshot: (): Promise<ContractSnapshot> => invoke('diagnostics_contract_snapshot'),
	themeGet: (): Promise<ThemeMode> => invoke('theme_get'),
	themeSet: (mode: ThemeMode, persist = true): Promise<void> =>
		invoke('theme_set', { mode, persist }),
	windowOpen: (label?: string): Promise<string> => invoke('window_open', { label: label ?? null }),
	fileProcess: (filepath: string, note?: string): Promise<string> =>
		invoke('file_process', { filepath, note: note ?? null }),
	trayUpdateLanguage: (lang: string): Promise<void> =>
		invoke('tray_update_language', { lang }),
};

/**
 * @returns whether the backend contract is reachable (false in a plain browser)
 */
export function backendContractAvailable(): boolean {
	return isTauri();
}
