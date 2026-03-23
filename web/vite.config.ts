import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		proxy: {
			'/api': {
				target: 'http://localhost:8765',
				changeOrigin: true,
			}
		}
	},
	build: {
		rollupOptions: {
			onwarn(warning, warn) {
				// rtf.js is intentionally large and lazy-loaded on demand — suppress the chunk size warning.
				if (warning.code === 'CHUNK_TOO_LARGE' && warning.message.includes('rtf.js')) return;
				warn(warning);
			}
		}
	}
});
