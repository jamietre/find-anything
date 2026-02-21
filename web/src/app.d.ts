declare global {
	namespace App {
		// Shape of $page.state, updated by pushState/replaceState in +page.svelte.
		// Kept intentionally empty so that AppState is assignable to it and
		// the reverse cast (PageState → AppState) is permitted by TypeScript.
		// eslint-disable-next-line @typescript-eslint/no-empty-object-type
		interface PageState {}
	}
}
export {};
