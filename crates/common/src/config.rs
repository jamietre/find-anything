use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub server: ServerConfig,
    pub sources: Vec<SourceConfig>,
    #[serde(default)]
    pub scan: ScanConfig,
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArchiveConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
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
