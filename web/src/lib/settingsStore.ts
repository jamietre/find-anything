import { writable } from 'svelte/store';

/** Lines shown before and after each match in search result cards (server-configured). */
export const contextWindow = writable(1);
