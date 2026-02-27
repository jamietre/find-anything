import hljs from 'highlight.js/lib/core';

// ── Language imports ──────────────────────────────────────────────────────────
import bash from 'highlight.js/lib/languages/bash';
import c from 'highlight.js/lib/languages/c';
import csharp from 'highlight.js/lib/languages/csharp';
import cpp from 'highlight.js/lib/languages/cpp';
import css from 'highlight.js/lib/languages/css';
import dockerfile from 'highlight.js/lib/languages/dockerfile';
import go from 'highlight.js/lib/languages/go';
import ini from 'highlight.js/lib/languages/ini'; // also used for TOML
import java from 'highlight.js/lib/languages/java';
import javascript from 'highlight.js/lib/languages/javascript';
import json from 'highlight.js/lib/languages/json';
import kotlin from 'highlight.js/lib/languages/kotlin';
import lua from 'highlight.js/lib/languages/lua';
import makefile from 'highlight.js/lib/languages/makefile';
import markdown from 'highlight.js/lib/languages/markdown';
import php from 'highlight.js/lib/languages/php';
import python from 'highlight.js/lib/languages/python';
import r from 'highlight.js/lib/languages/r';
import ruby from 'highlight.js/lib/languages/ruby';
import rust from 'highlight.js/lib/languages/rust';
import scala from 'highlight.js/lib/languages/scala';
import shell from 'highlight.js/lib/languages/shell';
import sql from 'highlight.js/lib/languages/sql';
import swift from 'highlight.js/lib/languages/swift';
import typescript from 'highlight.js/lib/languages/typescript';
import vim from 'highlight.js/lib/languages/vim';
import xml from 'highlight.js/lib/languages/xml'; // HTML, XML, SVG
import yaml from 'highlight.js/lib/languages/yaml';

hljs.registerLanguage('bash', bash);
hljs.registerLanguage('c', c);
hljs.registerLanguage('csharp', csharp);
hljs.registerLanguage('cpp', cpp);
hljs.registerLanguage('css', css);
hljs.registerLanguage('dockerfile', dockerfile);
hljs.registerLanguage('go', go);
hljs.registerLanguage('ini', ini);
hljs.registerLanguage('java', java);
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('json', json);
hljs.registerLanguage('kotlin', kotlin);
hljs.registerLanguage('lua', lua);
hljs.registerLanguage('makefile', makefile);
hljs.registerLanguage('markdown', markdown);
hljs.registerLanguage('php', php);
hljs.registerLanguage('python', python);
hljs.registerLanguage('r', r);
hljs.registerLanguage('ruby', ruby);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('scala', scala);
hljs.registerLanguage('shell', shell);
hljs.registerLanguage('sql', sql);
hljs.registerLanguage('swift', swift);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('vim', vim);
hljs.registerLanguage('xml', xml);
hljs.registerLanguage('yaml', yaml);

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
export function highlightFile(lines: string[], filePath: string): string {
	const lang = getLanguage(filePath);
	const code = lines.join('\n');

	try {
		if (lang && hljs.getLanguage(lang)) {
			return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
		}
		return hljs.highlightAuto(code).value;
	} catch {
		return escapeHtml(code);
	}
}

/**
 * Highlight a single line snippet for search result context.
 * Returns escaped HTML (no full-file context, so syntax may be approximate).
 */
export function highlightLine(content: string, filePath: string): string {
	const lang = getLanguage(filePath);
	try {
		if (lang && hljs.getLanguage(lang)) {
			return hljs.highlight(content, { language: lang, ignoreIllegals: true }).value;
		}
		return escapeHtml(content);
	} catch {
		return escapeHtml(content);
	}
}
