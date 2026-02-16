use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub server: ServerConfig,
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub watch: WatchConfig,
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

    #[serde(default = "default_max_file_size_kb")]
    pub max_file_size_kb: u64,

    #[serde(default)]
    pub follow_symlinks: bool,

    #[serde(default)]
    pub include_hidden: bool,

    #[serde(default)]
    pub ocr: bool,

    #[serde(default)]
    pub archives: ArchiveConfig,
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            exclude: default_excludes(),
            max_file_size_kb: default_max_file_size_kb(),
            follow_symlinks: false,
            include_hidden: false,
            ocr: false,
            archives: ArchiveConfig::default(),
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
}

impl Default for ArchiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_depth: default_max_archive_depth(),
        }
    }
}

fn default_max_archive_depth() -> usize {
    10
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
    ]
}

fn default_max_file_size_kb() -> u64 {
    1024
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerAppConfig {
    pub server: ServerAppSettings,
    #[serde(default)]
    pub search: SearchSettings,
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
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            default_limit: default_search_limit(),
            max_limit: default_max_limit(),
            fts_candidate_limit: default_fts_candidate_limit(),
        }
    }
}

fn default_search_limit() -> usize { 50 }
fn default_max_limit() -> usize { 500 }
fn default_fts_candidate_limit() -> usize { 2000 }

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
