/**
 * Parse image pixel dimensions from indexed metadata lines.
 *
 * Three tag families are recognised, in priority order:
 *  1. [EXIF:PixelXDimension] / [EXIF:PixelYDimension]  — actual rendered size (Exif 2.x)
 *  2. [EXIF:ImageWidth]      / [EXIF:ImageLength]       — TIFF-style dimensions
 *  3. [IMAGE:dimensions] WxH                            — our own basic-extractor fallback
 *
 * Returns null when no usable dimensions are found.
 */
export function parseImageDimensions(
	lines: { content: string }[]
): { width: number; height: number } | null {
	let pixelW: number | null = null;
	let pixelH: number | null = null;
	let imageW: number | null = null;
	let imageH: number | null = null;
	let basicW: number | null = null;
	let basicH: number | null = null;

	for (const l of lines) {
		let m: RegExpMatchArray | null;
		if ((m = l.content.match(/^\[EXIF:PixelXDimension\]\s+(\d+)/)))      pixelW = parseInt(m[1]);
		else if ((m = l.content.match(/^\[EXIF:PixelYDimension\]\s+(\d+)/))) pixelH = parseInt(m[1]);
		else if ((m = l.content.match(/^\[EXIF:ImageWidth\]\s+(\d+)/)))      imageW = parseInt(m[1]);
		else if ((m = l.content.match(/^\[EXIF:ImageLength\]\s+(\d+)/)))     imageH = parseInt(m[1]);
		else if ((m = l.content.match(/^\[IMAGE:dimensions\]\s+(\d+)x(\d+)/))) {
			basicW = parseInt(m[1]);
			basicH = parseInt(m[2]);
		}
	}

	const w = pixelW ?? imageW ?? basicW;
	const h = pixelH ?? imageH ?? basicH;
	return w && h ? { width: w, height: h } : null;
}
