const KEY = 'find_token';

export function getToken(): string {
	return localStorage.getItem(KEY) ?? '';
}

export function setToken(token: string): void {
	localStorage.setItem(KEY, token);
}

export function clearToken(): void {
	localStorage.removeItem(KEY);
}
