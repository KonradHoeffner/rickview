//! About page with stats about the package version and the loaded graph.
use crate::config::config;
use crate::rdf::{GraphEnum, graph, titles, types};
use bytesize::ByteSize;
use deepsize::DeepSizeOf;
use sophia::api::graph::Graph;
const VERSION: &str = env!("CARGO_PKG_VERSION");

use serde::Serialize;
#[derive(Serialize, Debug)]
pub struct About {
    pub cargo_pkg_version: &'static str,
    /// HTML with the build profile and the enabled/disabled features.
    pub build: String,
    pub kb: String,
    pub num_titles: usize,
    pub num_types: usize,
    pub titles_size: String,
    pub types_size: String,
    pub graph_size: Option<String>,
}

// features shown with their state, keep in sync with [features] in Cargo.toml (except "default")
const FEATURES: [(&str, bool); 2] = [("rdfxml", cfg!(feature = "rdfxml")), ("hdt", cfg!(feature = "hdt"))];

impl About {
    pub fn new() -> About {
        let graph_size = match graph() {
            #[cfg(feature = "hdt")]
            GraphEnum::HdtGraph(hdt_graph) => Some(ByteSize(hdt_graph.size_in_bytes() as u64).to_string()),
            GraphEnum::FastGraph(g) => Some(format!("~{} triples", g.triples().size_hint().0)),
        };
        let features = FEATURES
            .map(|(name, enabled)| {
                let (class, symbol) = if enabled { ("enabled", "✓") } else { ("disabled", "✗") };
                format!(r#"<span class="{class}" title="{class}">{symbol} {name}</span>"#)
            })
            .join(" ");
        About {
            cargo_pkg_version: VERSION,
            build: format!(
                r#"{} build, <a href="https://docs.rs/crate/rickview/latest/features" class="link">features</a> {features}"#,
                if cfg!(debug_assertions) { "debug" } else { "release" }
            ),
            kb: kb_description(),
            num_titles: titles().len(),
            num_types: types().len(),
            types_size: ByteSize(types().deep_size_of() as u64).to_string(),
            titles_size: ByteSize(titles().deep_size_of() as u64).to_string(),
            graph_size,
        }
    }
}

fn kb_description() -> String {
    let Some(path) = config().kb_file.as_deref() else { return "bundled example (TTL)".to_owned() };
    let kind = if std::fs::metadata(path).is_ok_and(|m| m.is_dir()) {
        "DIRECTORY".to_owned()
    } else {
        path.rsplit_once('.').map_or("UNKNOWN".to_owned(), |(_, extension)| extension.to_uppercase())
    };
    let mut escaped = String::new();
    tinytemplate::escape(path, &mut escaped);
    let path = if path.starts_with("http") { format!(r#"<a href="{escaped}" class="link">{escaped}</a>"#) } else { escaped };
    format!("{path} ({kind})")
}
