/**
 * Unified file path representation for files and archive members.
 *
 * Examples:
 * - Regular file: "src/main.rs"
 * - Archive member: "data.zip::readme.txt"
 * - Nested archive: "outer.zip::middle.zip::inner.zip::data.txt"
 */
export class FilePath {
	readonly full: string;
	readonly segments: string[];

	constructor(full: string) {
		this.full = full;
		this.segments = full.split('::');
	}

	/** First segment - the outermost file path */
	get outer(): string {
		return this.segments[0];
	}

	/** Everything after the first segment, joined with :: */
	get inner(): string | null {
		return this.segments.length > 1 ? this.segments.slice(1).join('::') : null;
	}

	/** True if this is a composite path (contains ::) */
	get isComposite(): boolean {
		return this.segments.length > 1;
	}

	/** The final segment - the actual file/member name */
	get filename(): string {
		return this.segments[this.segments.length - 1];
	}

	/** The parent directory/archive path, or null if at root */
	get parent(): FilePath | null {
		if (this.segments.length <= 1) return null;
		return new FilePath(this.segments.slice(0, -1).join('::'));
	}

	/** Join this path with a child segment */
	join(child: string): FilePath {
		return new FilePath(`${this.full}::${child}`);
	}

	/** Check if this path starts with another path */
	startsWith(other: FilePath | string): boolean {
		const otherFull = other instanceof FilePath ? other.full : other;
		return this.full.startsWith(otherFull);
	}

	/** Check if this path equals another */
	equals(other: FilePath | string | null): boolean {
		if (other === null) return false;
		const otherFull = other instanceof FilePath ? other.full : other;
		return this.full === otherFull;
	}

	/** Serialize to string for URL/storage */
	toString(): string {
		return this.full;
	}

	/** Parse from string (static factory) */
	static parse(path: string | null): FilePath | null {
		return path ? new FilePath(path) : null;
	}

	/** Create from separate outer and inner paths */
	static fromParts(outer: string, inner: string | null): FilePath {
		return new FilePath(inner ? `${outer}::${inner}` : outer);
	}
}
