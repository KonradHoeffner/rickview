use config::{ConfigError, Environment, File, FileFormat};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use sophia::iri::Iri;
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read};
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Debug)]
pub struct Header {
    pub title: Option<String>,
    pub subtitle: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// Server-side common path prefix (scope) that normally matches the last part of the namespace but may also be "" e.g. for local testing.
    /// For example, port 8080, empty base (default) and namespace <http://ab.com/d/> would serve <http://ab.com/d/X> at <localhost:8080/X>.
    /// Base "d" would serve <http://ab.com/d/X> at localhost:8080/d/X.
    /// Don't use a trailing slash, it will be removed.
    /// See also <https://docs.rs/actix-web/latest/actix_web/struct.Scope.html>.
    pub base: String,
    pub header: Header,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub kb_file: Option<String>,
    pub port: u16,
    pub github: Option<String>,
    pub prefix: Box<str>,
    #[serde(with = "iri_serde")]
    pub namespace: Iri<Box<str>>,
    pub namespaces: HashMap<Box<str>, Box<str>>,
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
    /// if data/body.html is present, it is inserted into index.html on rendering
    pub body: Option<String>,
    // override CSS, for example the font
    pub css: Option<String>,
    /// disable memory and CPU intensive preprocessing on large knowledge bases
    pub large: bool,
}

mod iri_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use sophia::iri::Iri;
    pub fn serialize<S>(namespace: &Iri<Box<str>>, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        serializer.serialize_str(namespace.as_str())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Iri<Box<str>>, D::Error>
    where D: Deserializer<'de> {
        let s = Box::<str>::deserialize(deserializer)?;
        Iri::new(s).map_err(serde::de::Error::custom)
    }
}

// path relative to source file
static DEFAULT: &str = std::include_str!("../data/default.toml");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

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
        if !config.base.is_empty() && !config.base.starts_with('/') {
            eprintln!("Warning: Non-empty base path '{}' does not start with a leading '/'.", config.base);
        }
        if config.base.ends_with('/') {
            config.base.pop();
        }
        // log level precedence: env var > config key > default value
        let mut binding = env_logger::Builder::new();
        let builder = match std::env::var("RUST_LOG") {
            Err(_) => binding.filter(
                Some("rickview"),
                log::LevelFilter::from_str(config.log_level.as_ref().unwrap_or(&"info".to_owned())).unwrap_or(log::LevelFilter::Info),
            ),
            _ => &mut env_logger::Builder::from_default_env(),
        };

        let _ = builder.format_timestamp(None).format_target(false).try_init();

        // path relative to executable
        match std::fs::File::open("data/body.html") {
            Ok(body_file) => {
                let mut br = BufReader::new(body_file);
                let mut s = String::new();
                match br.read_to_string(&mut s) {
                    Ok(_) => config.body = Some(s),
                    Err(e) => error!("Cannot read data/body.html: {e:?}"),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!("data/body.html does not exist, skipping.");
            }
            Err(e) => error!("Cannot open data/body.html: {e:?}"),
        }

        Ok(config)
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();
pub fn config() -> &'static Config { CONFIG.get_or_init(|| Config::new().expect("Error reading configuration.")) }
