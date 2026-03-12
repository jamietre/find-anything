import { writable } from 'svelte/store';

/** Lines shown before and after each match in search result cards (server-configured). */
export const contextWindow = writable(1);

/** Maximum markdown file size (KB) the UI will render as formatted HTML (server-configured). */
export const maxMarkdownRenderKb = writable(512);
