import hljs from 'highlight.js/lib/core';
import type { LanguageFn } from 'highlight.js';

// ── Lazy language loaders ─────────────────────────────────────────────────────

type Loader = () => Promise<{ default: LanguageFn }>;

const LANG_LOADERS: Record<string, Loader> = {
	bash:       () => import('highlight.js/lib/languages/bash'),
	c:          () => import('highlight.js/lib/languages/c'),
	csharp:     () => import('highlight.js/lib/languages/csharp'),
	cpp:        () => import('highlight.js/lib/languages/cpp'),
	css:        () => import('highlight.js/lib/languages/css'),
	dockerfile: () => import('highlight.js/lib/languages/dockerfile'),
	go:         () => import('highlight.js/lib/languages/go'),
	ini:        () => import('highlight.js/lib/languages/ini'),
	java:       () => import('highlight.js/lib/languages/java'),
	javascript: () => import('highlight.js/lib/languages/javascript'),
	json:       () => import('highlight.js/lib/languages/json'),
	kotlin:     () => import('highlight.js/lib/languages/kotlin'),
	lua:        () => import('highlight.js/lib/languages/lua'),
	makefile:   () => import('highlight.js/lib/languages/makefile'),
	markdown:   () => import('highlight.js/lib/languages/markdown'),
	php:        () => import('highlight.js/lib/languages/php'),
	python:     () => import('highlight.js/lib/languages/python'),
	r:          () => import('highlight.js/lib/languages/r'),
	ruby:       () => import('highlight.js/lib/languages/ruby'),
	rust:       () => import('highlight.js/lib/languages/rust'),
	scala:      () => import('highlight.js/lib/languages/scala'),
	shell:      () => import('highlight.js/lib/languages/shell'),
	sql:        () => import('highlight.js/lib/languages/sql'),
	swift:      () => import('highlight.js/lib/languages/swift'),
	typescript: () => import('highlight.js/lib/languages/typescript'),
	vim:        () => import('highlight.js/lib/languages/vim'),
	xml:        () => import('highlight.js/lib/languages/xml'),
	yaml:       () => import('highlight.js/lib/languages/yaml'),
};

const loaded = new Set<string>();

async function ensureLanguage(lang: string): Promise<void> {
	if (loaded.has(lang)) return;
	const loader = LANG_LOADERS[lang];
	if (!loader) return;
	const mod = await loader();
	hljs.registerLanguage(lang, mod.default);
	loaded.add(lang);
}

// ── Extension → language map ──────────────────────────────────────────────────

const EXT_MAP: Record<string, string> = {
	// Systems
	rs: 'rust',
	c: 'c',
	h: 'c',
	cpp: 'cpp',
	cxx: 'cpp',
	cc: 'cpp',
	hpp: 'cpp',
	cs: 'csharp',
	// JVM
	java: 'java',
	kt: 'kotlin',
	kts: 'kotlin',
	scala: 'scala',
	// Web
	js: 'javascript',
	mjs: 'javascript',
	cjs: 'javascript',
	ts: 'typescript',
	tsx: 'typescript',
	jsx: 'javascript',
	css: 'css',
	html: 'xml',
	htm: 'xml',
	xml: 'xml',
	svg: 'xml',
	// Scripting
	py: 'python',
	rb: 'ruby',
	lua: 'lua',
	php: 'php',
	r: 'r',
	// Go
	go: 'go',
	// Swift
	swift: 'swift',
	// Shell
	sh: 'bash',
	bash: 'bash',
	zsh: 'shell',
	fish: 'shell',
	ps1: 'shell',
	// Data / config
	json: 'json',
	yaml: 'yaml',
	yml: 'yaml',
	toml: 'ini',
	ini: 'ini',
	cfg: 'ini',
	conf: 'ini',
	env: 'ini',
	// Markup
	md: 'markdown',
	markdown: 'markdown',
	// SQL
	sql: 'sql',
	// Docker
	dockerfile: 'dockerfile',
	// Build
	makefile: 'makefile',
	mk: 'makefile',
	// Vim
	vim: 'vim',
};

export function getLanguage(filePath: string): string | null {
	const base = filePath.split('/').pop() ?? filePath;
	// Handle "Dockerfile", "Makefile" etc. (no extension)
	const lower = base.toLowerCase();
	if (lower === 'dockerfile') return 'dockerfile';
	if (lower === 'makefile') return 'makefile';

	const ext = base.split('.').pop()?.toLowerCase() ?? '';
	return EXT_MAP[ext] ?? null;
}

function escapeHtml(text: string): string {
	return text
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;');
}

/**
 * Highlight a full file. Returns the highlighted HTML as a single string
 * suitable for rendering inside a <pre><code> block.
 * Rendering as a single block preserves multi-line spans (strings, comments).
 */
export async function highlightFile(lines: string[], filePath: string): Promise<string> {
	const lang = getLanguage(filePath);
	const code = lines.join('\n');

	try {
		if (lang) {
			await ensureLanguage(lang);
			if (hljs.getLanguage(lang)) {
				return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
			}
		}
		return escapeHtml(code);
	} catch {
		return escapeHtml(code);
	}
}

/**
 * Highlight a single line snippet for search result context.
 * Returns escaped HTML (no full-file context, so syntax may be approximate).
 */
export async function highlightLine(content: string, filePath: string): Promise<string> {
	const lang = getLanguage(filePath);
	try {
		if (lang) {
			await ensureLanguage(lang);
			if (hljs.getLanguage(lang)) {
				return hljs.highlight(content, { language: lang, ignoreIllegals: true }).value;
			}
		}
		return escapeHtml(content);
	} catch {
		return escapeHtml(content);
	}
}
