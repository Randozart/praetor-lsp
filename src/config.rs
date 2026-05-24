use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct IntentConfig {
    pub enabled: bool,
    pub severity: String,
    pub exempt_patterns: Vec<String>,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            severity: "error".into(),
            exempt_patterns: vec![
                "fn get_.*".into(),
                "fn set_.*".into(),
                "fn new\\(".into(),
                "fn main\\(".into(),
                "fn test_.*".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct ComplexityConfig {
    pub big_o_threshold: String,
    pub cyclomatic_max: u32,
    pub cognitive_max: u32,
    pub max_function_lines: u32,
    pub max_nesting_depth: u32,
    pub max_params: u32,
}

impl Default for ComplexityConfig {
    fn default() -> Self {
        Self {
            big_o_threshold: "O(n²)".into(),
            cyclomatic_max: 15,
            cognitive_max: 15,
            max_function_lines: 50,
            max_nesting_depth: 4,
            max_params: 6,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    pub enabled: bool,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LspConfig {
    pub extensions: Vec<String>,
    pub exclude_extensions: Vec<String>,
}

impl Default for LspConfig {
    fn default() -> Self {
        Self {
            extensions: vec!["*".into()],
            exclude_extensions: vec![
                ".md".into(),
                ".txt".into(),
                ".json".into(),
                ".yaml".into(),
                ".toml".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct FormalVerificationConfig {
    pub auto_discover: bool,
    pub disable: Vec<String>,
}

impl Default for FormalVerificationConfig {
    fn default() -> Self {
        Self {
            auto_discover: true,
            disable: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct StateGraphConfig {
    pub enabled: bool,
    pub path: String,
}

impl Default for StateGraphConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            path: ".praetor/state-graph.json".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct DatalogConfig {
    pub auth_functions: Vec<String>,
    pub private_data_labels: Vec<String>,
    pub entry_points: Vec<String>,
    pub log_functions: Vec<String>,
}

impl Default for DatalogConfig {
    fn default() -> Self {
        Self {
            auth_functions: vec!["authenticate".into()],
            private_data_labels: vec!["private_data".into()],
            entry_points: vec!["main".into(), "run".into()],
            log_functions: vec!["log_access".into()],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default)]
pub struct PraetorConfig {
    pub intent: IntentConfig,
    pub complexity: ComplexityConfig,
    pub security: SecurityConfig,
    pub lsp: LspConfig,
    pub formal_verification: FormalVerificationConfig,
    pub state_graph: StateGraphConfig,
    pub datalog: DatalogConfig,
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl PraetorConfig {
    /// Discover .praetor.toml starting from cwd, walking up.
    pub fn discover() -> Option<Self> {
        let cwd = std::env::current_dir().ok()?;
        let mut dir: &Path = &cwd;

        loop {
            let candidate = dir.join(".praetor.toml");
            if candidate.is_file() {
                let contents = std::fs::read_to_string(&candidate).ok()?;
                let mut cfg: Self = toml::from_str(&contents).ok()?;
                cfg.path = Some(candidate);
                return Some(cfg);
            }
            dir = dir.parent()?;
        }
    }
}
