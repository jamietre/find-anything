use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::warn;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub server: ServerConfig,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub watch: WatchConfig,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub url: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceConfig {
    pub name: String,
    pub paths: Vec<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanConfig {
    #[serde(default = "default_excludes")]
    pub exclude: Vec<String>,

    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,

    #[serde(default)]
    pub follow_symlinks: bool,

    #[serde(default)]
    pub include_hidden: bool,

    #[serde(default)]
    pub archives: ArchiveConfig,

    /// Maximum line length (in characters) for PDF text extraction.
    /// Lines longer than this are split at word boundaries so that context
    /// retrieval returns meaningful snippets.
    /// Set to 0 to disable wrapping. Default: 120.
    #[serde(default = "default_max_line_length")]
    pub max_line_length: usize,

    /// Name of the marker file that signals a directory (and all descendants)
    /// should be excluded from indexing. Default: ".noindex".
    #[serde(default = "default_noindex_file")]
    pub noindex_file: String,

    /// Name of the per-directory config file that overrides scan settings for
    /// a subtree. Default: ".index".
    #[serde(default = "default_index_file")]
    pub index_file: String,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            exclude: default_excludes(),
            max_file_size_mb: default_max_file_size_mb(),
            follow_symlinks: false,
            include_hidden: false,
            archives: ArchiveConfig::default(),
            max_line_length: default_max_line_length(),
            noindex_file: default_noindex_file(),
            index_file: default_index_file(),
        }
    }
}

impl ScanConfig {
    /// Produce a new `ScanConfig` by applying a per-directory override.
    ///
    /// - `exclude` is **additive**: patterns are appended to the parent list.
    /// - All other fields are **replacement**: the innermost value wins.
    /// - `noindex_file` and `index_file` are never overridden (global-only).
    pub fn apply_override(&self, ov: &ScanOverride) -> ScanConfig {
        let mut result = self.clone();
        if let Some(extra) = &ov.exclude {
            result.exclude.extend(extra.iter().cloned());
        }
        if let Some(v) = ov.max_file_size_mb {
            result.max_file_size_mb = v;
        }
        if let Some(v) = ov.include_hidden {
            result.include_hidden = v;
        }
        if let Some(v) = ov.follow_symlinks {
            result.follow_symlinks = v;
        }
        if let Some(v) = ov.max_line_length {
            result.max_line_length = v;
        }
        if let Some(arch_ov) = &ov.archives {
            if let Some(v) = arch_ov.enabled {
                result.archives.enabled = v;
            }
            if let Some(v) = arch_ov.max_depth {
                result.archives.max_depth = v;
            }
        }
        result
    }
}

/// Partial scan config read from a per-directory `.index` file.
/// All fields are optional; `None` means "inherit from parent config".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanOverride {
    /// Additional exclude patterns (appended to parent list, never removed).
    pub exclude: Option<Vec<String>>,
    pub max_file_size_mb: Option<u64>,
    pub include_hidden: Option<bool>,
    pub follow_symlinks: Option<bool>,
    pub archives: Option<ArchiveOverride>,
    pub max_line_length: Option<usize>,
}

/// Archive-specific fields for a `ScanOverride`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArchiveOverride {
    pub enabled: Option<bool>,
    pub max_depth: Option<usize>,
}

/// Load and parse a `.index` override file from `dir`.
/// Returns `None` if the file is absent, unreadable, or unparseable.
pub fn load_dir_override(dir: &Path, index_filename: &str) -> Option<ScanOverride> {
    let path = dir.join(index_filename);
    let content = std::fs::read_to_string(&path).ok()?;
    match toml::from_str::<ScanOverride>(&content) {
        Ok(ov) => Some(ov),
        Err(e) => {
            warn!("invalid {} file at {}: {e}", index_filename, path.display());
            None
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum nesting depth for archives-within-archives.
    /// Prevents infinite recursion from malicious zip bombs.
    /// Default: 10. Set to 1 to only extract direct members (no nested archives).
    #[serde(default = "default_max_archive_depth")]
    pub max_depth: usize,
    /// Maximum size in MB of a temporary file created when extracting a nested
    /// 7z or large nested zip archive.  Guards against excessive disk use from
    /// deeply compressed or unusually large inner archives.  Default: 500 MB.
    #[serde(default = "default_max_archive_temp_file_mb")]
    pub max_temp_file_mb: usize,
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: default_max_archive_depth(),
            max_temp_file_mb: default_max_archive_temp_file_mb(),
        }
    }
}

fn default_max_archive_depth() -> usize {
    10
}

fn default_max_archive_temp_file_mb() -> usize {
    500
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchConfig {
    /// Milliseconds to wait after last event before processing the batch.
    #[serde(default = "default_debounce_ms")]
    pub debounce_ms: u64,

    /// Directory containing find-extract-* binaries.
    /// None = auto-detect (same dir as find-watch, then PATH).
    #[serde(default)]
    pub extractor_dir: Option<String>,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            debounce_ms: default_debounce_ms(),
            extractor_dir: None,
        }
    }
}

fn default_debounce_ms() -> u64 {
    500
}

fn default_excludes() -> Vec<String> {
    vec![
        // ── Development artefacts ─────────────────────────────────────────────
        "**/.git/**".into(),
        "**/node_modules/**".into(),
        "**/target/**".into(),
        "**/__pycache__/**".into(),
        "**/.next/**".into(),
        "**/dist/**".into(),
        "**/.cache/**".into(),
        "**/.tox/**".into(),
        "**/.venv/**".into(),
        "**/venv/**".into(),
        "**/*.pyc".into(),
        "**/*.class".into(),
        // ── Version control ───────────────────────────────────────────────────
        "**/.svn/**".into(),
        "**/.hg/**".into(),
        // ── Synology NAS ─────────────────────────────────────────────────────
        "**/#recycle/**".into(),
        "**/@eaDir/**".into(),
        "**/#snapshot/**".into(),
        // ── Linux virtual/runtime filesystems ────────────────────────────────
        // No "**/" prefix: these are root-level only and we don't want to
        // accidentally exclude a user's ~/proc or ~/tmp when they scan their
        // home directory.  When scanning from /, "proc/**" matches /proc/…
        // but not /home/user/proc/….
        //
        // /etc, /usr/lib/systemd, /usr/share and similar config-bearing paths
        // are intentionally NOT excluded so that service unit files, sysctl
        // settings, and other configuration is discoverable.
        "proc/**".into(),    // virtual process filesystem — gigabytes of pseudo-files
        "sys/**".into(),     // kernel sysfs — hardware/driver state, no real data
        "dev/**".into(),     // device nodes — no searchable content
        "run/**".into(),     // runtime (PIDs, sockets, volatile tempfiles)
        "tmp/**".into(),     // temporary files
        "var/tmp/**".into(), // persistent temporary files
        "var/lock/**".into(), // lock files
        "var/run/**".into(), // legacy symlink of /run on modern systems
        // ── Linux binary directories ──────────────────────────────────────────
        // Contain only executables / shared libraries; no text config lives here.
        // Again no "**/" prefix so they only match at the scan root (i.e. / or
        // a container rootfs), not inside a user's home checkout or data share.
        "bin/**".into(),
        "sbin/**".into(),
        "lib/**".into(),
        "lib64/**".into(),
        "usr/bin/**".into(),
        "usr/sbin/**".into(),
        "usr/libexec/**".into(),
        "usr/lib/debug/**".into(), // DWARF debug symbols — large, binary
        // ── macOS ─────────────────────────────────────────────────────────────
        "**/__MACOSX/**".into(),
        "**/.Spotlight-V100/**".into(),
        "**/.Trashes/**".into(),
        "**/.fseventsd/**".into(),
        "**/Library/Caches/**".into(),
        // ── Windows system directories ────────────────────────────────────────
        // Use "**/" prefix because Windows trees appear nested inside backup
        // shares (e.g. backups/2024/Windows/System32/…).  The two-level
        // specificity avoids false positives on innocent directories.
        "**/$RECYCLE.BIN/**".into(),
        "**/System Volume Information/**".into(),
        "**/Windows/System32/**".into(),
        "**/Windows/SysWOW64/**".into(),
        "**/Windows/WinSxS/**".into(),     // side-by-side assemblies — huge
        "**/Windows/Installer/**".into(),
        "**/Windows/SoftwareDistribution/**".into(), // Windows Update cache
        "**/Windows/Temp/**".into(),
        "**/AppData/Local/Temp/**".into(),
        // ── Linux misc ────────────────────────────────────────────────────────
        "**/lost+found/**".into(),
    ]
}

fn default_max_file_size_mb() -> u64 {
    10
}

fn default_max_line_length() -> usize {
    120
}

fn default_noindex_file() -> String {
    ".noindex".into()
}

fn default_index_file() -> String {
    ".index".into()
}

fn default_true() -> bool {
    true
}

/// Configuration passed to extractor functions.
///
/// Bundles all per-extraction settings into one struct so that adding new
/// options in the future only requires updating this struct and its
/// construction site — not every function signature in the call chain.
///
/// Construction sites that don't care about a particular field can use
/// `..ExtractorConfig::default()` to forward-compatibly inherit the defaults.
#[derive(Debug, Clone, Copy)]
pub struct ExtractorConfig {
    /// Maximum file/member size in KB; content extraction is skipped above this.
    pub max_size_kb: usize,
    /// Maximum archive nesting depth; prevents zip-bomb recursion.
    pub max_depth: usize,
    /// Maximum line length in characters for PDF extraction.
    /// Long lines are wrapped at word boundaries. 0 = no wrapping.
    pub max_line_length: usize,
    /// Maximum size in MB of a temporary file used when extracting nested 7z
    /// archives (which require a seekable file path) or oversized nested zips.
    /// Guards against excessive disk use. Default: 500 MB.
    pub max_temp_file_mb: usize,
    /// When false (default), archive members whose path contains a dot-prefixed
    /// component (e.g. `.terraform/`, `.git/`) are skipped entirely, consistent
    /// with the filesystem walk's `include_hidden = false` behaviour.
    pub include_hidden: bool,
}

impl Default for ExtractorConfig {
    fn default() -> Self {
        Self {
            max_size_kb: 10 * 1024,
            max_depth: default_max_archive_depth(),
            max_line_length: default_max_line_length(),
            max_temp_file_mb: default_max_archive_temp_file_mb(),
            include_hidden: false,
        }
    }
}

impl ExtractorConfig {
    /// Build an `ExtractorConfig` from the scan section of the client config.
    pub fn from_scan(scan: &ScanConfig) -> Self {
        Self {
            max_size_kb: scan.max_file_size_mb as usize * 1024,
            max_depth: scan.archives.max_depth,
            max_line_length: scan.max_line_length,
            max_temp_file_mb: scan.archives.max_temp_file_mb,
            include_hidden: scan.include_hidden,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAppConfig {
    pub server: ServerAppSettings,
    #[serde(default)]
    pub search: SearchSettings,
    #[serde(default)]
    pub log: LogConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAppSettings {
    #[serde(default = "default_bind")]
    pub bind: String,
    pub data_dir: String,
    pub token: String,
}

fn default_bind() -> String {
    "127.0.0.1:8080".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchSettings {
    #[serde(default = "default_search_limit")]
    pub default_limit: usize,
    #[serde(default = "default_max_limit")]
    pub max_limit: usize,
    #[serde(default = "default_fts_candidate_limit")]
    pub fts_candidate_limit: usize,
    /// Number of lines shown before and after each match in search result cards.
    /// Total lines displayed = 2 × context_window + 1. Default: 1 (3 lines total).
    #[serde(default = "default_context_window")]
    pub context_window: usize,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            default_limit: default_search_limit(),
            max_limit: default_max_limit(),
            fts_candidate_limit: default_fts_candidate_limit(),
            context_window: default_context_window(),
        }
    }
}

fn default_search_limit() -> usize { 50 }
fn default_max_limit() -> usize { 500 }
fn default_fts_candidate_limit() -> usize { 2000 }
fn default_context_window() -> usize { 1 }

// ── Log config ────────────────────────────────────────────────────────────────

/// Logging configuration shared by client and server.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LogConfig {
    /// Regular-expression patterns for log messages to suppress.
    /// Any event whose message contains a match for one of these patterns is
    /// silently dropped before it reaches the output formatter.
    /// Default: suppresses "unknown glyph name" noise from pdf-extract.
    #[serde(default = "default_log_ignore")]
    pub ignore: Vec<String>,
}

fn default_log_ignore() -> Vec<String> {
    vec!["pdf_extract: unknown glyph name".to_string()]
}

/// Resolves the server config path using the following priority:
///
/// 1. `FIND_ANYTHING_SERVER_CONFIG` environment variable (if set)
/// 2. `$XDG_CONFIG_HOME/find-anything/server.toml` (if `XDG_CONFIG_HOME` is set)
/// 3. `/etc/find-anything/server.toml` if running as root (uid 0) — typical for system services
/// 4. `~/.config/find-anything/server.toml` otherwise
pub fn default_server_config_path() -> String {
    if let Ok(p) = std::env::var("FIND_ANYTHING_SERVER_CONFIG") {
        return p;
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return format!("{xdg}/find-anything/server.toml");
    }
    // Running as root → system-wide config location used by service units.
    #[cfg(unix)]
    if unsafe { libc::getuid() } == 0 {
        return "/etc/find-anything/server.toml".into();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/find-anything/server.toml")
}

/// Resolves the client config path using the following priority:
///
/// 1. `FIND_ANYTHING_CONFIG` environment variable (if set)
/// 2. `$XDG_CONFIG_HOME/find-anything/client.toml` (if `XDG_CONFIG_HOME` is set)
/// 3. `/etc/find-anything/client.toml` (when running as root, e.g. system service)
/// 4. `~/.config/find-anything/client.toml` (default)
pub fn default_config_path() -> String {
    if let Ok(p) = std::env::var("FIND_ANYTHING_CONFIG") {
        return p;
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        return format!("{xdg}/find-anything/client.toml");
    }
    // Running as root → system-wide config location used by service units.
    #[cfg(unix)]
    if unsafe { libc::getuid() } == 0 {
        return "/etc/find-anything/client.toml".into();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    format!("{home}/.config/find-anything/client.toml")
}

// ── Config loaders with unknown-field warnings ─────────────────────────────

/// Parse a client `client.toml` string, emitting `warn!` for any unrecognised keys.
pub fn parse_client_config(toml_str: &str) -> Result<ClientConfig> {
    let value: toml::Value = toml::from_str(toml_str).context("invalid TOML")?;
    let mut unknown = Vec::new();
    let cfg = serde_ignored::deserialize(value, |path| {
        unknown.push(path.to_string());
    })
    .context("parsing client config")?;
    for key in &unknown {
        warn!("unknown config key: {key}");
    }
    Ok(cfg)
}

/// Parse a server `server.toml` string, emitting `warn!` for any unrecognised keys.
pub fn parse_server_config(toml_str: &str) -> Result<ServerAppConfig> {
    let value: toml::Value = toml::from_str(toml_str).context("invalid TOML")?;
    let mut unknown = Vec::new();
    let cfg = serde_ignored::deserialize(value, |path| {
        unknown.push(path.to_string());
    })
    .context("parsing server config")?;
    for key in &unknown {
        warn!("unknown config key: {key}");
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_config_default_values() {
        let w = WatchConfig::default();
        assert_eq!(w.debounce_ms, 500);
        assert!(w.extractor_dir.is_none());
    }

    #[test]
    fn watch_config_serde_missing_fields_use_defaults() {
        // A config with no [watch] section should deserialise to defaults.
        let w: WatchConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(w.debounce_ms, 500);
        assert!(w.extractor_dir.is_none());
    }

    #[test]
    fn watch_config_serde_explicit_values() {
        let w: WatchConfig =
            serde_json::from_str(r#"{"debounce_ms":200,"extractor_dir":"/usr/local/bin"}"#)
                .unwrap();
        assert_eq!(w.debounce_ms, 200);
        assert_eq!(w.extractor_dir.as_deref(), Some("/usr/local/bin"));
    }

    #[test]
    fn scan_config_default_control_file_names() {
        let s = ScanConfig::default();
        assert_eq!(s.noindex_file, ".noindex");
        assert_eq!(s.index_file, ".index");
    }

    #[test]
    fn scan_override_exclude_is_additive() {
        let base = ScanConfig {
            exclude: vec!["**/.git/**".into()],
            ..ScanConfig::default()
        };
        let ov = ScanOverride {
            exclude: Some(vec!["*.log".into()]),
            ..ScanOverride::default()
        };
        let result = base.apply_override(&ov);
        assert_eq!(result.exclude, vec!["**/.git/**", "*.log"]);
    }

    #[test]
    fn scan_override_replaces_scalar_fields() {
        let base = ScanConfig::default();
        let ov = ScanOverride {
            include_hidden: Some(true),
            max_file_size_mb: Some(99),
            ..ScanOverride::default()
        };
        let result = base.apply_override(&ov);
        assert!(result.include_hidden);
        assert_eq!(result.max_file_size_mb, 99);
        // noindex_file/index_file are inherited unchanged
        assert_eq!(result.noindex_file, ".noindex");
    }

    #[test]
    fn scan_override_archive_fields() {
        let base = ScanConfig::default();
        let ov = ScanOverride {
            archives: Some(ArchiveOverride { enabled: Some(false), max_depth: Some(2) }),
            ..ScanOverride::default()
        };
        let result = base.apply_override(&ov);
        assert!(!result.archives.enabled);
        assert_eq!(result.archives.max_depth, 2);
        assert_eq!(result.archives.max_temp_file_mb, 500); // unchanged
    }

    #[test]
    fn scan_override_toml_parses() {
        let toml = r#"
include_hidden = true
exclude = ["*.tmp"]

[archives]
enabled = false
"#;
        let ov: ScanOverride = toml::from_str(toml).unwrap();
        assert_eq!(ov.include_hidden, Some(true));
        assert_eq!(ov.exclude, Some(vec!["*.tmp".into()]));
        assert_eq!(ov.archives.as_ref().unwrap().enabled, Some(false));
    }

    #[test]
    fn client_config_watch_field_defaults_when_absent() {
        // Simulate a client.toml that has no [watch] section.
        let json = r#"{
            "server": {"url": "http://localhost:8080", "token": "t"},
            "sources": []
        }"#;
        let cfg: ClientConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.watch.debounce_ms, 500);
        assert!(cfg.watch.extractor_dir.is_none());
    }
}
