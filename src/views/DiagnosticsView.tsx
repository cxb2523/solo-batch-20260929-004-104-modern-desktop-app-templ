import {
	Badge,
	Button,
	Code,
	Group,
	Loader,
	SimpleGrid,
	Stack,
	Table,
	Text,
	Title,
} from '@mantine/core';
import { notifications } from '@mantine/notifications';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
	ContractSnapshot,
	ThemeMode,
	THEME_CHANGED_EVENT,
	backendContractAvailable,
	contractApi,
} from '../tauri/contract';
import { listen } from '@tauri-apps/api/event';

const LAYER_COLOR: Record<string, string> = {
	domain: 'blue',
	crosscutting: 'grape',
};

function verdictColor(verdict: string): string {
	return verdict === 'allowed' ? 'teal' : 'red';
}

/**
 * Line-by-line comparison between the reflected registry order and the list
 * the backend hands us. Both originate from the same snapshot, so any drift is
 * a contract bug; we surface the first mismatch.
 */
function diffSnapshot(snapshot: ContractSnapshot): string[] {
	const mismatches: string[] = [];
	const reflected = snapshot.commands.map(command => command.name);
	if (reflected.length !== snapshot.commandNames.length) {
		mismatches.push(
			`count drift: ${reflected.length} rows vs ${snapshot.commandNames.length} names`,
		);
	}
	reflected.forEach((name, index) => {
		if (name !== snapshot.commandNames[index]) {
			mismatches.push(`row ${index + 1}: "${name}" != "${snapshot.commandNames[index]}"`);
		}
	});
	if (reflected.length !== snapshot.commandCount) {
		mismatches.push(`commandCount ${snapshot.commandCount} != reflected rows ${reflected.length}`);
	}
	if (!snapshot.registryConsistent) mismatches.push('registryConsistent=false');
	if (!snapshot.acl.uniformAcrossWindows) mismatches.push('window permission sets differ');
	return mismatches;
}

export default function DiagnosticsView() {
	const { t } = useTranslation();
	const available = backendContractAvailable();
	const [snapshot, setSnapshot] = useState<ContractSnapshot | null>(null);
	const [error, setError] = useState<string | null>(null);
	const [themeMode, setThemeMode] = useState<ThemeMode | null>(null);

	const refresh = useCallback(async () => {
		try {
			const next = await contractApi.snapshot();
			setSnapshot(next);
			setError(null);
		} catch (snapshotError) {
			setError(String(snapshotError));
		}
	}, []);

	useEffect(() => {
		if (!available) return;
		refresh();
		contractApi.themeGet().then(setThemeMode).catch(console.error);
		const unlistenPromise = listen<{ mode: ThemeMode }>(THEME_CHANGED_EVENT, event =>
			setThemeMode(event.payload.mode),
		);
		const interval = window.setInterval(refresh, 2000);
		return () => {
			window.clearInterval(interval);
			unlistenPromise.then(unlisten => unlisten());
		};
	}, [available, refresh]);

	async function openWindow() {
		try {
			const label = await contractApi.windowOpen();
			notifications.show({ title: t('Window opened'), message: label });
			await refresh();
		} catch (openError) {
			notifications.show({
				color: 'red',
				title: 'window_open failed',
				message: String(openError),
			});
		}
	}

	async function setTheme(mode: ThemeMode) {
		try {
			await contractApi.themeSet(mode);
			setThemeMode(mode);
		} catch (themeError) {
			notifications.show({ color: 'red', title: 'theme_set failed', message: String(themeError) });
		}
	}

	if (!available) {
		return (
			<Stack>
				<Title order={3}>{t('Diagnostics Panel')}</Title>
				<Text>{t('Backend contract is only available inside the desktop app.')}</Text>
			</Stack>
		);
	}

	if (error) {
		return (
			<Stack>
				<Title order={3}>{t('Diagnostics Panel')}</Title>
				<Text c='red'>{error}</Text>
				<Button w={200} onClick={refresh}>{t('Refresh')}</Button>
			</Stack>
		);
	}

	if (!snapshot) return <Loader />;

	const mismatches = diffSnapshot(snapshot);
	const everyCommandAllowed = snapshot.commands.every(command => command.aclVerdict === 'allowed');

	return (
		<Stack>
			<Group justify='space-between'>
				<Title order={3}>{t('Diagnostics Panel')}</Title>
				<Group>
					<Button variant='light' onClick={refresh}>{t('Refresh')}</Button>
					<Button onClick={openWindow}>{t('Open Another Window')}</Button>
				</Group>
			</Group>

			<SimpleGrid cols={{ base: 1, sm: 3 }}>
				<Group gap='xs'>
					<Text fw={700}>{t('Command count')}:</Text>
					<Code data-testid='command-count'>{snapshot.commandCount}</Code>
				</Group>
				<Group gap='xs'>
					<Text fw={700}>{t('Duplicate policy')}:</Text>
					<Badge>{snapshot.policy}</Badge>
				</Group>
				<Group gap='xs'>
					<Text fw={700}>{t('Theme')}:</Text>
					<Badge data-testid='theme-mode'>{themeMode ?? '…'}</Badge>
					<Button size='compact-xs' variant='subtle' onClick={() => setTheme('light')}>{t('Light')}</Button>
					<Button size='compact-xs' variant='subtle' onClick={() => setTheme('dark')}>{t('Dark')}</Button>
					<Button size='compact-xs' variant='subtle' onClick={() => setTheme('auto')}>{t('Auto')}</Button>
				</Group>
			</SimpleGrid>

			<Group>
				<Badge
					color={mismatches.length === 0 ? 'teal' : 'red'}
					data-testid='registry-match'
				>
					{mismatches.length === 0
						? t('Panel rows match backend snapshot one-by-one')
						: t('Snapshot mismatch')}
				</Badge>
				<Badge color={snapshot.acl.uniformAcrossWindows ? 'teal' : 'red'} data-testid='acl-uniform'>
					{snapshot.acl.uniformAcrossWindows
						? t('All windows share the same permission set')
						: t('Window permission sets differ')}
				</Badge>
				<Badge color={everyCommandAllowed ? 'teal' : 'red'} data-testid='acl-verdicts'>
					{t('ACL verdicts from runtime reflection')}
				</Badge>
			</Group>

			{mismatches.length > 0 && (
				<Code block data-testid='snapshot-diff'>{mismatches.join('\n')}</Code>
			)}

			<Table striped withTableBorder data-testid='command-table'>
				<Table.Thead>
					<Table.Tr>
						<Table.Th>#</Table.Th>
						<Table.Th>{t('Command')}</Table.Th>
						<Table.Th>{t('Domain')}</Table.Th>
						<Table.Th>{t('Layer')}</Table.Th>
						<Table.Th>{t('Params')}</Table.Th>
						<Table.Th>{t('Returns')}</Table.Th>
						<Table.Th>{t('ACL')}</Table.Th>
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{snapshot.commands.map((command, index) => (
						<Table.Tr key={command.name} data-command-name={command.name}>
							<Table.Td>{index + 1}</Table.Td>
							<Table.Td>
								<Text component='span' fw={600}>{command.name}</Text>
								<Text component='div' size='xs' c='dimmed'>{command.summary}</Text>
							</Table.Td>
							<Table.Td><Code>{command.domain}</Code></Table.Td>
							<Table.Td>
								<Badge color={LAYER_COLOR[command.layer] ?? 'gray'} variant='light'>
									{command.layer}
								</Badge>
							</Table.Td>
							<Table.Td>
								{command.params.length === 0 ? (
									<Text size='xs' c='dimmed'>—</Text>
								) : (
									command.params.map(param => (
										<Code key={param.name} mr={4}>
											{param.name}
											{param.required ? '' : '?'}: {param.type}
										</Code>
									))
								)}
							</Table.Td>
							<Table.Td><Code>{command.returns}</Code></Table.Td>
							<Table.Td>
								<Group gap={4}>
									<Badge color={verdictColor(command.aclVerdict)} variant='dot'>
										{command.aclVerdict}
									</Badge>
									{command.requiredAcl.map(acl => (
										<Code key={acl}>{acl}</Code>
									))}
								</Group>
							</Table.Td>
						</Table.Tr>
					))}
				</Table.Tbody>
			</Table>

			<Title order={5}>{t('Effective permissions per window')}</Title>
			<Table data-testid='window-table'>
				<Table.Thead>
					<Table.Tr>
						<Table.Th>{t('Window')}</Table.Th>
						<Table.Th>{t('Permission count')}</Table.Th>
						<Table.Th>{t('Permissions')}</Table.Th>
					</Table.Tr>
				</Table.Thead>
				<Table.Tbody>
					{snapshot.acl.windows.map(grant => (
						<Table.Tr key={grant.label} data-window-label={grant.label}>
							<Table.Td><Code>{grant.label}</Code></Table.Td>
							<Table.Td>{grant.permissions.length}</Table.Td>
							<Table.Td>
								<Text size='xs'>{grant.permissions.join(', ')}</Text>
							</Table.Td>
						</Table.Tr>
					))}
				</Table.Tbody>
			</Table>
		</Stack>
	);
}
