export interface KindOption {
	value: string;
	label: string;
	/** Server kind values to send for this option (defaults to [value]). */
	serverKinds?: string[];
}

export interface KindGroup {
	label?: string;
	kinds: KindOption[];
}

export const KIND_GROUPS: KindGroup[] = [
	{
		label: 'Documents',
		kinds: [
			{ value: 'pdf',      label: 'PDF' },
			{ value: 'text',     label: 'Text' },
			{ value: 'document', label: 'Office' },
			{ value: 'code',     label: 'Code' },
			{ value: 'epub',     label: 'eBook' },
		],
	},
	{
		label: 'Media',
		kinds: [
			{ value: 'image', label: 'Image', serverKinds: ['image', 'dicom'] },
			{ value: 'audio', label: 'Audio' },
			{ value: 'video', label: 'Video' },
		],
	},
	{
		label: 'Other',
		kinds: [
			{ value: 'archive', label: 'Archive' },
			{ value: 'binary',  label: 'Binary' },
		],
	},
];

/** Flat list of all options, for lookup by value. */
export const KIND_OPTIONS: KindOption[] = KIND_GROUPS.flatMap(g => g.kinds);

/**
 * Expand UI kind values to server kind values.
 * e.g. 'image' → ['image', 'dicom'] so DICOM files appear under Image.
 */
export function expandKindsForServer(kinds: string[]): string[] {
	return kinds.flatMap(k => {
		const opt = KIND_OPTIONS.find(o => o.value === k);
		return opt?.serverKinds ?? [k];
	});
}
