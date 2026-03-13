import { writable } from 'svelte/store';
import type { Readable } from 'svelte/store';
import { getToken } from './token';

export interface LiveEvent {
	source: string;
	path: string;
	action: 'added' | 'modified' | 'deleted' | 'renamed';
	new_path?: string;
	indexed_at: number;
}

const _store = writable<LiveEvent | null>(null);

/** Readable store emitting each live index event as it arrives. Null when disconnected. */
export const liveEvent: Readable<LiveEvent | null> = { subscribe: _store.subscribe };

/**
 * Start the SSE connection. Returns a stop() function.
 * Call once from the root component's onMount; call stop() in onDestroy.
 */
export function startLiveUpdates(): () => void {
	let stopped = false;
	let abort: AbortController | null = null;

	async function connect(backoff: number): Promise<void> {
		if (stopped) return;
		if (backoff > 0) {
			await new Promise<void>((r) => setTimeout(r, backoff));
		}
		if (stopped) return;

		const token = getToken();
		if (!token) {
			// No token yet — retry after 2 s
			setTimeout(() => connect(2000), 0);
			return;
		}

		const connectionStartTime = Date.now() / 1000;
		abort = new AbortController();

		try {
			const resp = await fetch('/api/v1/recent/stream', {
				headers: { Authorization: `Bearer ${token}` },
				signal: abort.signal,
			});

			if (!resp.ok || !resp.body) {
				// Server error or no body — reconnect with backoff
				if (!stopped) setTimeout(() => connect(Math.max(1000, Math.min(backoff * 2, 30000))), 0);
				return;
			}

			const reader = resp.body.getReader();
			const decoder = new TextDecoder();
			let buffer = '';
			let pending = '';

			while (true) {
				const { done, value } = await reader.read();
				if (done) break;

				buffer += decoder.decode(value, { stream: true });
				const parts = buffer.split('\n');
				// Keep the last (possibly incomplete) line in buffer
				buffer = parts.pop() ?? '';

				for (const line of parts) {
					if (line.startsWith('data: ')) {
						pending = line.slice(6);
					} else if (line === '') {
						// Blank line = end of SSE event
						if (pending) {
							try {
								const ev = JSON.parse(pending) as LiveEvent;
								// Skip historical snapshot — only emit genuinely new events
								if (ev.indexed_at >= connectionStartTime) {
									_store.set(ev);
								}
							} catch {
								// ignore malformed JSON
							}
							pending = '';
						}
					}
					// Lines starting with ':' (heartbeat) or 'event:' are ignored
				}
			}

			// Stream ended cleanly — reconnect quickly
			if (!stopped) setTimeout(() => connect(0), 500);
		} catch {
			if (!stopped) setTimeout(() => connect(Math.max(1000, Math.min(backoff * 2, 30000))), 0);
		}
	}

	connect(0);

	return () => {
		stopped = true;
		abort?.abort();
	};
}
