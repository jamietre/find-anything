import { describe, it, expect } from 'vitest';
import { parseImageDimensions } from './imageMeta';

const lines = (contents: string[]) => contents.map((content) => ({ content }));

describe('parseImageDimensions', () => {
	it('returns null for empty lines', () => {
		expect(parseImageDimensions([])).toBeNull();
	});

	it('returns null when no dimension tags are present', () => {
		expect(parseImageDimensions(lines(['[EXIF:Make] Apple', '[TAG:artist] Unknown']))).toBeNull();
	});

	it('parses [IMAGE:dimensions] WxH', () => {
		expect(parseImageDimensions(lines(['[IMAGE:dimensions] 826x1093']))).toEqual({
			width: 826,
			height: 1093
		});
	});

	it('parses [EXIF:ImageWidth] / [EXIF:ImageLength]', () => {
		expect(
			parseImageDimensions(lines(['[EXIF:ImageWidth] 487', '[EXIF:ImageLength] 217']))
		).toEqual({ width: 487, height: 217 });
	});

	it('parses [EXIF:PixelXDimension] / [EXIF:PixelYDimension]', () => {
		expect(
			parseImageDimensions(lines(['[EXIF:PixelXDimension] 811', '[EXIF:PixelYDimension] 777']))
		).toEqual({ width: 811, height: 777 });
	});

	it('prefers PixelXDimension over ImageWidth over IMAGE:dimensions', () => {
		const mixed = lines([
			'[IMAGE:dimensions] 100x200',
			'[EXIF:ImageWidth] 300',
			'[EXIF:ImageLength] 400',
			'[EXIF:PixelXDimension] 811',
			'[EXIF:PixelYDimension] 777'
		]);
		expect(parseImageDimensions(mixed)).toEqual({ width: 811, height: 777 });
	});

	it('falls back to ImageWidth when PixelXDimension absent', () => {
		const mixed = lines([
			'[IMAGE:dimensions] 100x200',
			'[EXIF:ImageWidth] 487',
			'[EXIF:ImageLength] 217'
		]);
		expect(parseImageDimensions(mixed)).toEqual({ width: 487, height: 217 });
	});

	it('falls back to IMAGE:dimensions when no EXIF dimension tags present', () => {
		const mixed = lines(['[EXIF:Make] Canon', '[IMAGE:dimensions] 826x1093']);
		expect(parseImageDimensions(mixed)).toEqual({ width: 826, height: 1093 });
	});

	it('handles mixed width/height sources (PixelX + ImageLength fallback)', () => {
		// Unusual but possible: only one axis present per family
		const mixed = lines(['[EXIF:PixelXDimension] 811', '[EXIF:ImageLength] 217']);
		expect(parseImageDimensions(mixed)).toEqual({ width: 811, height: 217 });
	});

	it('ignores non-integer values', () => {
		// Tags with non-numeric values should not match
		expect(parseImageDimensions(lines(['[EXIF:ImageWidth] abc']))).toBeNull();
	});
});
