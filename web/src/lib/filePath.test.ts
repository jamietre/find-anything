import { describe, it, expect } from 'vitest';
import { FilePath, splitEntryPath, childListPrefix, shouldExpandEntry } from './filePath';

// ── FilePath class ────────────────────────────────────────────────────────────

describe('FilePath', () => {
	describe('regular (non-composite) paths', () => {
		it('outer equals full path', () => {
			expect(new FilePath('src/main.rs').outer).toBe('src/main.rs');
		});

		it('inner is null', () => {
			expect(new FilePath('src/main.rs').inner).toBeNull();
		});

		it('isComposite is false', () => {
			expect(new FilePath('src/main.rs').isComposite).toBe(false);
		});

		it('filename is the last path component', () => {
			expect(new FilePath('src/main.rs').filename).toBe('src/main.rs');
		});

		it('parent is null at root', () => {
			expect(new FilePath('file.txt').parent).toBeNull();
		});

		it('toString returns full path', () => {
			expect(new FilePath('src/main.rs').toString()).toBe('src/main.rs');
		});
	});

	describe('composite paths (archive members)', () => {
		it('outer is the archive path', () => {
			expect(new FilePath('archive.zip::member.txt').outer).toBe('archive.zip');
		});

		it('inner is the member path', () => {
			expect(new FilePath('archive.zip::member.txt').inner).toBe('member.txt');
		});

		it('isComposite is true', () => {
			expect(new FilePath('archive.zip::member.txt').isComposite).toBe(true);
		});

		it('filename is the final segment', () => {
			expect(new FilePath('archive.zip::member.txt').filename).toBe('member.txt');
		});

		it('parent strips the last segment', () => {
			const p = new FilePath('archive.zip::member.txt').parent;
			expect(p?.full).toBe('archive.zip');
		});
	});

	describe('triple-nested paths', () => {
		const path = new FilePath('outer.zip::middle.zip::inner.zip::data.txt');

		it('outer is the outermost archive', () => {
			expect(path.outer).toBe('outer.zip');
		});

		it('inner contains all remaining segments joined by ::', () => {
			expect(path.inner).toBe('middle.zip::inner.zip::data.txt');
		});

		it('filename is the innermost file', () => {
			expect(path.filename).toBe('data.txt');
		});

		it('parent is the path without the last segment', () => {
			expect(path.parent?.full).toBe('outer.zip::middle.zip::inner.zip');
		});

		it('segments has four entries', () => {
			expect(path.segments).toHaveLength(4);
		});
	});

	describe('join', () => {
		it('appends a child segment with ::', () => {
			const p = new FilePath('archive.zip');
			expect(p.join('member.txt').full).toBe('archive.zip::member.txt');
		});

		it('works for deeply nested paths', () => {
			const p = new FilePath('a.zip::b.zip');
			expect(p.join('c.txt').full).toBe('a.zip::b.zip::c.txt');
		});
	});

	describe('startsWith', () => {
		it('true when path starts with given string', () => {
			expect(new FilePath('a.zip::b.txt').startsWith('a.zip')).toBe(true);
		});

		it('false when path does not start with given string', () => {
			expect(new FilePath('a.zip::b.txt').startsWith('b.zip')).toBe(false);
		});
	});

	describe('equals', () => {
		it('true for identical path strings', () => {
			expect(new FilePath('a.zip::b.txt').equals('a.zip::b.txt')).toBe(true);
		});

		it('false for different paths', () => {
			expect(new FilePath('a.zip::b.txt').equals('a.zip::c.txt')).toBe(false);
		});

		it('false for null', () => {
			expect(new FilePath('a.zip').equals(null)).toBe(false);
		});
	});

	describe('static factories', () => {
		it('parse returns null for null input', () => {
			expect(FilePath.parse(null)).toBeNull();
		});

		it('parse returns FilePath for non-null input', () => {
			expect(FilePath.parse('a.zip::b.txt')?.full).toBe('a.zip::b.txt');
		});

		it('fromParts with inner', () => {
			expect(FilePath.fromParts('a.zip', 'b.txt').full).toBe('a.zip::b.txt');
		});

		it('fromParts with null inner', () => {
			expect(FilePath.fromParts('a.zip', null).full).toBe('a.zip');
		});
	});
});

// ── splitEntryPath ────────────────────────────────────────────────────────────

describe('splitEntryPath', () => {
	it('plain path returns only path, no archivePath', () => {
		const result = splitEntryPath('src/main.rs');
		expect(result.path).toBe('src/main.rs');
		expect(result.archivePath).toBeUndefined();
	});

	it('composite path splits at first ::', () => {
		const result = splitEntryPath('archive.zip::member.txt');
		expect(result.path).toBe('archive.zip');
		expect(result.archivePath).toBe('member.txt');
	});

	it('doubly-nested path: path is outer, archivePath is the rest', () => {
		const result = splitEntryPath('outer.zip::middle.zip::data.txt');
		expect(result.path).toBe('outer.zip');
		expect(result.archivePath).toBe('middle.zip::data.txt');
	});

	it('archive member inside a subdir', () => {
		const result = splitEntryPath('archive.zip::docs/readme.txt');
		expect(result.path).toBe('archive.zip');
		expect(result.archivePath).toBe('docs/readme.txt');
	});
});

// ── childListPrefix ───────────────────────────────────────────────────────────
//
// When expanding a tree entry, the UI calls:
//   listDir(source, childListPrefix(entry))
// which maps to GET /api/v1/tree?prefix=<result>.

describe('childListPrefix', () => {
	it('directory entry: prefix is the path as-is (already has trailing /)', () => {
		const entry = { kind: undefined, path: 'src/' };
		expect(childListPrefix(entry)).toBe('src/');
	});

	it('archive entry: prefix appends :: to the path', () => {
		const entry = { kind: 'archive' as const, path: 'data.zip' };
		expect(childListPrefix(entry)).toBe('data.zip::');
	});

	it('nested archive entry inside another archive', () => {
		const entry = { kind: 'archive' as const, path: 'outer.zip::inner.zip' };
		expect(childListPrefix(entry)).toBe('outer.zip::inner.zip::');
	});

	it('non-archive file kind does not append ::', () => {
		const entry = { kind: 'text' as const, path: 'notes/' };
		expect(childListPrefix(entry)).toBe('notes/');
	});
});

// ── shouldExpandEntry ─────────────────────────────────────────────────────────
//
// Controls which tree nodes auto-expand when a file is selected.

describe('shouldExpandEntry', () => {
	describe('directory entries', () => {
		const dir = { entry_type: 'dir' as const, path: 'src/' };

		it('expands when activePath is inside the directory', () => {
			expect(shouldExpandEntry(dir, 'src/main.rs')).toBe(true);
		});

		it('expands when activePath is in a nested subdir', () => {
			expect(shouldExpandEntry(dir, 'src/utils/helper.rs')).toBe(true);
		});

		it('does not expand when activePath is a sibling directory', () => {
			expect(shouldExpandEntry(dir, 'tests/main.rs')).toBe(false);
		});

		it('does not expand when activePath matches path prefix but not a child', () => {
			// "src_extra/file.rs" starts with "src" but not with "src/"
			const narrowDir = { entry_type: 'dir' as const, path: 'src/' };
			expect(shouldExpandEntry(narrowDir, 'src_extra/file.rs')).toBe(false);
		});
	});

	describe('archive entries', () => {
		const archive = { entry_type: 'file' as const, path: 'data.zip' };

		it('expands when activePath is the archive itself', () => {
			expect(shouldExpandEntry(archive, 'data.zip')).toBe(true);
		});

		it('expands when activePath is a direct member', () => {
			expect(shouldExpandEntry(archive, 'data.zip::member.txt')).toBe(true);
		});

		it('expands when activePath is a member in a subdir of the archive', () => {
			expect(shouldExpandEntry(archive, 'data.zip::docs/readme.txt')).toBe(true);
		});

		it('does not expand for a different archive', () => {
			expect(shouldExpandEntry(archive, 'other.zip::member.txt')).toBe(false);
		});

		it('does not expand when path shares a prefix but is not the same archive', () => {
			// "data.zip_backup::file" starts with "data.zip" but not "data.zip::"
			expect(shouldExpandEntry(archive, 'data.zip_backup::file')).toBe(false);
		});
	});

	describe('nested archive entries', () => {
		const innerArchive = { entry_type: 'file' as const, path: 'outer.zip::inner.zip' };

		it('expands when activePath is a member of the inner archive', () => {
			expect(shouldExpandEntry(innerArchive, 'outer.zip::inner.zip::data.txt')).toBe(true);
		});

		it('expands when activePath is the inner archive itself', () => {
			expect(shouldExpandEntry(innerArchive, 'outer.zip::inner.zip')).toBe(true);
		});

		it('does not expand for a sibling in the outer archive', () => {
			expect(shouldExpandEntry(innerArchive, 'outer.zip::other.zip::data.txt')).toBe(false);
		});
	});
});
