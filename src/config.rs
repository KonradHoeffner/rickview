use config::{ConfigError, Environment, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub base_path: String,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub kb_file: String,
    pub port: u16,
    pub github: Option<String>,
    pub prefix: String,
    pub namespace: String,
    pub namespaces: HashMap<String, String>,
    pub examples: Vec<String>,
    pub title_properties: Vec<String>,
    pub type_properties: Vec<String>,
    pub description_properties: HashSet<String>,
    pub homepage: Option<String>,
    pub endpoint: Option<String>,
    pub doc: Option<String>,
    pub log_level: Option<String>,
}

static DEFAULT: &str = std::include_str!("../data/default.toml");

impl Config {
    pub fn new() -> Result<Self, ConfigError> {
        let config = config::Config::builder()
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
            .build()?
            .try_deserialize();
        if config.base_path.ends_with('/') {
            config.base_path.pop();
        }
        config
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::new().expect("Error reading configuration.");
}
