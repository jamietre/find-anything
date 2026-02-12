import { env } from '$env/dynamic/private';
import type { RequestHandler } from './$types';

/**
 * Transparent proxy to find-server.
 * Adds the Authorization: Bearer header so the token never reaches the browser.
 *
 * Browser calls  → /api/v1/search?q=…
 * Proxy forwards → ${FIND_SERVER_URL}/api/v1/search?q=…
 */
async function proxyRequest(event: Parameters<RequestHandler>[0]): Promise<Response> {
	const { params, url, request } = event;
	const serverUrl = env.FIND_SERVER_URL ?? 'http://localhost:8765';
	const token = env.FIND_TOKEN ?? '';

	const targetUrl = `${serverUrl}/api/${params.path}${url.search}`;

	const headers = new Headers();
	headers.set('Authorization', `Bearer ${token}`);

	const ct = request.headers.get('content-type');
	if (ct) headers.set('content-type', ct);

	let body: ArrayBuffer | undefined;
	if (request.method !== 'GET' && request.method !== 'HEAD') {
		body = await request.arrayBuffer();
	}

	const upstream = await fetch(targetUrl, {
		method: request.method,
		headers,
		body
	});

	const respHeaders = new Headers();
	const upstreamCt = upstream.headers.get('content-type');
	if (upstreamCt) respHeaders.set('content-type', upstreamCt);

	return new Response(upstream.body, {
		status: upstream.status,
		statusText: upstream.statusText,
		headers: respHeaders
	});
}

export const GET: RequestHandler = (event) => proxyRequest(event);
export const POST: RequestHandler = (event) => proxyRequest(event);
export const PUT: RequestHandler = (event) => proxyRequest(event);
export const DELETE: RequestHandler = (event) => proxyRequest(event);
export const PATCH: RequestHandler = (event) => proxyRequest(event);
