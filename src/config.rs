use config::{ConfigError, Environment, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub base: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub kb_file: Option<String>,
    pub port: u16,
    pub github: Option<String>,
    pub prefix: String,
    pub namespace: String,
    pub namespaces: HashMap<String, String>,
    pub examples: Vec<String>,
    pub title_properties: Vec<String>,
    pub type_properties: Vec<String>,
    pub description_properties: HashSet<String>,
    pub langs: Vec<String>,
    pub homepage: Option<String>,
    pub endpoint: Option<String>,
    /// Show inverse triples, which use the given URI as object instead of subject. May be slow on very large kbs.
    pub show_inverse: bool,
    /// When false, knowledge base will only be loaded on first resource (non-index) access.
    pub doc: Option<String>,
    pub log_level: Option<String>,
    pub cargo_pkg_version: String,
}

static DEFAULT: &str = std::include_str!("../data/default.toml");
const VERSION: &str = env!("CARGO_PKG_VERSION");

impl Config {
    pub fn new() -> Result<Self, ConfigError> {
        let mut config: Config = config::Config::builder()
            .add_source(File::from_str(DEFAULT, FileFormat::Toml))
            .add_source(File::new("data/config.toml", FileFormat::Toml).required(false))
            .add_source(
                Environment::with_prefix("rickview")
                    .try_parsing(true)
                    .list_separator(" ")
                    .with_list_parse_key("examples")
                    .with_list_parse_key("title_properties")
                    .with_list_parse_key("type_properties"),
            )
            .set_override("cargo_pkg_version", VERSION)?
            .build()?
            .try_deserialize()?;
        if config.base.ends_with('/') {
            config.base.pop();
        }
        Ok(config)
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();
pub fn config() -> &'static Config { CONFIG.get_or_init(|| Config::new().expect("Error reading configuration.")) }
