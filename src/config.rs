use config::{ConfigError, Environment, File, FileFormat};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub kb_file: String,
    pub namespace: String,
    pub namespaces: HashMap<String, String>,
    pub type_properties: HashSet<String>,
    pub description_properties: HashSet<String>,
}

impl Config {
    pub fn new() -> Result<Self, ConfigError> {
        config::Config::builder()
            .add_source(File::new("data/default.toml", FileFormat::Toml))
            .add_source(File::new("data/config.toml", FileFormat::Toml).required(false))
            .add_source(Environment::with_prefix("rickview"))
            .build()?
            .try_deserialize()
    }
}

lazy_static! {
    pub static ref CONFIG: Config = Config::new().expect("Error reading configuration.");
}
