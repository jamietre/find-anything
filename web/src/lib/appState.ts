import { FilePath } from './filePath';
import type { LineSelection } from './lineSelection';
import { parseHash } from './lineSelection';

export type View = 'results' | 'file';
export type PanelMode = 'file' | 'dir';

export interface AppState {
	view: View;
	query: string;
	mode: string;
	selectedSources: string[];
	fileSource: string;
	currentFile: FilePath | null;
	fileSelection: LineSelection;
	panelMode: PanelMode;
	currentDirPrefix: string;
}

// Serializable form stored in history.state via SvelteKit's pushState/replaceState.
// FilePath class instances don't survive structured clone (prototype getters are lost),
// so currentFile is stored as its string path and reconstructed on restore.
export type SerializedAppState = Omit<AppState, 'currentFile'> & {
	currentFilePath: string | null;
};

export function serializeState(s: AppState): SerializedAppState {
	const { currentFile, ...rest } = s;
	return { ...rest, currentFilePath: currentFile?.full ?? null };
}

export function deserializeState(s: SerializedAppState): AppState {
	return { ...s, currentFile: s.currentFilePath ? new FilePath(s.currentFilePath) : null };
}

export function buildUrl(s: AppState): string {
	const p = new URLSearchParams();
	if (s.query) p.set('q', s.query);
	if (s.mode && s.mode !== 'fuzzy') p.set('mode', s.mode);
	s.selectedSources.forEach((src) => p.append('source', src));
	if (s.view === 'file' && s.currentFile) {
		p.set('view', 'file');
		p.set('fsource', s.fileSource);
		p.set('path', s.currentFile.outer);
		if (s.currentFile.inner) p.set('apath', s.currentFile.inner);
		if (s.panelMode === 'dir') {
			p.set('panel', 'dir');
			p.set('dir', s.currentDirPrefix);
		}
	}
	const qs = p.toString();
	return qs ? `?${qs}` : location.pathname;
}

export function restoreFromParams(
	params: URLSearchParams
): AppState & { showTree: boolean } {
	const v = (params.get('view') ?? 'results') as View;
	const path = params.get('path');
	const apath = params.get('apath');
	return {
		view: v,
		query: params.get('q') ?? '',
		mode: params.get('mode') ?? 'fuzzy',
		selectedSources: params.getAll('source'),
		fileSource: params.get('fsource') ?? '',
		currentFile: path ? FilePath.fromParts(path, apath) : null,
		fileSelection: parseHash(location.hash),
		panelMode: (params.get('panel') ?? 'file') as PanelMode,
		currentDirPrefix: params.get('dir') ?? '',
		showTree: v === 'file',
	};
}
