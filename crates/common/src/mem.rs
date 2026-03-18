pub use find_extract_types::mem::available_bytes;

/// Format a byte count as a human-readable string (e.g. `"2.4 GB"`, `"512 KB"`).
pub fn fmt_bytes(n: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.1} KB", n as f64 / KB as f64)
    } else {
        format!("{} B", n)
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_bytes;

    #[test]
    fn test_fmt_bytes() {
        assert_eq!(fmt_bytes(0),                    "0 B");
        assert_eq!(fmt_bytes(1),                    "1 B");
        assert_eq!(fmt_bytes(1023),                 "1023 B");
        assert_eq!(fmt_bytes(1024),                 "1.0 KB");
        assert_eq!(fmt_bytes(1536),                 "1.5 KB");
        assert_eq!(fmt_bytes(1024 * 1024 - 1),      "1024.0 KB");
        assert_eq!(fmt_bytes(1024 * 1024),          "1.0 MB");
        assert_eq!(fmt_bytes(1024 * 1024 * 512),    "512.0 MB");
        assert_eq!(fmt_bytes(1024 * 1024 * 1024),   "1.0 GB");
        assert_eq!(fmt_bytes(2_553_268_401),        "2.4 GB"); // matches the log example
    }
}
