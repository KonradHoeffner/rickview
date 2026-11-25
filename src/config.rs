#[cfg(feature = "server")]
use config::Environment;
use config::{ConfigError, File, FileFormat};
#[cfg(feature = "server")]
use log::{debug, error};
use serde::{Deserialize, Serialize};
use sophia::iri::Iri;
use std::collections::{HashMap, HashSet};
#[cfg(feature = "server")]
use std::io::{BufReader, Read};
#[cfg(feature = "server")]
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    /// Server-side common path prefix (scope) that normally matches the last part of the namespace but may also be "" e.g. for local testing.
    /// For example, port 8080, empty base (default) and namespace <http://ab.com/d/> would serve <http://ab.com/d/X> at <localhost:8080/X>.
    /// Base "d" would serve <http://ab.com/d/X> at localhost:8080/d/X.
    /// Don't use a trailing slash, it will be removed.
    /// See also <https://docs.rs/actix-web/latest/actix_web/struct.Scope.html>.
    pub base: String,
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
    /// Whether OWL axioms are shown: requires the `owl` feature, a non-large knowledge base and a single file or URL in a supported RDF format.
    #[serde(default)]
    pub owl: bool,
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
    /// Build a configuration for the client-side viewer from the default values and a TOML string.
    pub fn from_toml(toml: &str) -> Result<Self, ConfigError> {
        let mut config: Config = config::Config::builder()
            .add_source(File::from_str(DEFAULT, FileFormat::Toml))
            .add_source(File::from_str(toml, FileFormat::Toml))
            .set_override("cargo_pkg_version", VERSION)?
            .build()?
            .try_deserialize()?;
        config.owl = false;
        Ok(Self::normalize(config))
    }

    #[cfg(feature = "server")]
    pub fn new() -> Result<Self, ConfigError> {
        // configuration precedence: env var > config key > default value
        // namespaces cannot be configured with env vars
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
        config = Self::normalize(config);
        // initialize logging here because we want it as early as possible but we need the log level
        let mut binding = env_logger::Builder::new();
        let builder = match std::env::var("RUST_LOG") {
            Err(_) => binding.filter(
                Some("rickview"),
                log::LevelFilter::from_str(config.log_level.as_ref().unwrap_or(&"info".to_owned())).unwrap_or(log::LevelFilter::Info),
            ),
            _ => &mut env_logger::Builder::from_default_env(),
        };

        let _ = builder.format_timestamp(None).format_target(false).try_init();

        // optional custom HTML included only in the index.html template
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

        config.owl = cfg!(feature = "owl")
            && !config.large
            && match config.kb_file.as_deref() {
                None => true,
                Some(path) => {
                    let file = std::path::Path::new(path);
                    (file.is_file() || path.contains("://"))
                        && !file.is_dir()
                        && matches!(
                            file.extension().and_then(std::ffi::OsStr::to_str).map(str::to_ascii_lowercase).as_deref(),
                            Some("ttl" | "nt" | "rdf" | "owl")
                        )
                }
            };

        Ok(config)
    }

    fn normalize(mut config: Self) -> Self {
        if !config.base.is_empty() && !config.base.starts_with('/') && config.base != "#" {
            eprintln!("Warning: Non-empty base path '{}' does not start with a leading '/'.", config.base);
        }
        if config.base.ends_with('/') {
            config.base.pop();
        }
        config
    }
}

static CONFIG: OnceLock<Config> = OnceLock::new();

/// Initialize the configuration of the client-side viewer. Must be called before rendering.
pub fn set(config: Config) -> Result<(), Config> { CONFIG.set(config) }

#[cfg(feature = "server")]
pub fn config() -> &'static Config { CONFIG.get_or_init(|| Config::new().expect("Error reading configuration.")) }

#[cfg(not(feature = "server"))]
pub fn config() -> &'static Config { CONFIG.get().expect("Configuration not initialized; call config::set first.") }
