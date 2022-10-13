//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::{config::config, resource::Resource};
use log::*;
use multimap::MultiMap;
#[cfg(feature = "rdfxml")]
use sophia::serializer::xml::RdfXmlSerializer;
use sophia::{
    graph::{inmem::sync::FastGraph, *},
    iri::{error::InvalidIri, AsIri, Iri, IriBox},
    ns::Namespace,
    parser::{nt, turtle},
    prefix::{PrefixBox, PrefixMap},
    serializer::{
        nt::NtSerializer,
        turtle::{TurtleConfig, TurtleSerializer},
        Stringifier, TripleSerializer,
    },
    term,
    term::{RefTerm, TTerm, Term::*},
    triple::{stream::TripleSource, Triple},
};
use std::{collections::HashMap, fmt, fs::File, io::BufReader, path::Path, sync::OnceLock, time::Instant};

static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");

fn get_prefixed_pair(iri: &Iri) -> Option<(String, String)> {
    let (p, s) = prefixes().get_prefixed_pair(iri)?;
    Some((p.to_string(), s.to_string()))
}

struct Piri {
    iri: IriBox,
    prefixed: Option<(String, String)>,
}

/// convert sophia::term::iri::Iri sophia::iri::IriBox
/// major sophia refactoring on the way, this function will hopefully be unnecessary in the next sophia version is finished
impl<TD: term::TermData> From<&term::iri::Iri<TD>> for Piri {
    fn from(tiri: &term::iri::Iri<TD>) -> Self { Piri::new(IriBox::new_unchecked(tiri.value().to_string().into_boxed_str())) }
}

impl fmt::Display for Piri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.iri.value()) }
}

impl Piri {
    fn from_suffix(suffix: &str) -> Self { Piri::new(IriBox::new_unchecked((config().namespace.clone() + suffix).into_boxed_str())) }
    fn new(iri: IriBox) -> Self { Self { prefixed: get_prefixed_pair(&iri.as_iri()), iri } }
    fn embrace(&self) -> String { format!("&lt;{}&gt;", self) }
    fn prefixed_string(&self, bold: bool, embrace: bool) -> String {
        if let Some((p, s)) = &self.prefixed {
            if bold {
                format!("{p}:<b>{s}</b>")
            } else {
                format!("{p}:{s}")
            }
        } else if embrace {
            self.embrace()
        } else {
            self.to_string()
        }
    }
    fn short(&self) -> String { self.prefixed_string(false, false) }

    fn root_relative(&self) -> String { self.iri.value().replace(&config().namespace, &(config().base.clone() + "/")) }
    fn property_anchor(&self) -> String { format!("<a href='{}'>{}</a>", self.root_relative(), self.prefixed_string(true, false)) }
}

/// Load RDF graph from the RDF Turtle file specified in the config.
fn graph() -> &'static FastGraph {
    GRAPH.get_or_init(|| {
        let t = Instant::now();
        let triples = match &config().kb_file {
            None => {
                warn!("No knowledge base configured. Loading example knowledge base. Set kb_file in data/config.toml or env var RICKVIEW_KB_FILE.");
                turtle::parse_str(EXAMPLE_KB).collect_triples()
            }
            Some(filename) => match File::open(filename) {
                Err(e) => {
                    error!("Cannot open knowledge base '{}': {}. Check kb_file in data/config.toml or env var RICKVIEW_KB_FILE.", filename, e);
                    std::process::exit(1);
                }
                Ok(file) => {
                    let reader = BufReader::new(file);
                    let triples = match Path::new(&filename).extension().and_then(|p| p.to_str()) {
                        Some("ttl") => turtle::parse_bufread(reader).collect_triples(),
                        Some("nt") => nt::parse_bufread(reader).collect_triples(),
                        // error: returns HdtGraph but FastGraph expected, use trait object
                        #[cfg(feature = "hdt")]
                        Some("hdt") => {
                            return hdt::HdtGraph::new(hdt::Hdt::new(file).unwrap());
                        }
                        x => {
                            error!("Unknown extension: \"{:?}\": cannot parse knowledge base. Aborting.", x);
                            std::process::exit(1);
                        }
                    };
                    triples
                }
            },
        };
        let g: FastGraph = triples.unwrap_or_else(|x| {
            error!("Unable to parse knowledge base {}: {}", &config().kb_file.as_deref().unwrap_or("example"), x);
            std::process::exit(1);
        });
        if log_enabled!(Level::Debug) {
            info!("~{} FastGraph triples from {} in {:?}", g.triples().size_hint().0, &config().kb_file.as_deref().unwrap_or("example kb"), t.elapsed());
        }
        g
    })
}

/// (prefix,iri) pairs from the config
fn prefixes() -> &'static Vec<(PrefixBox, IriBox)> {
    PREFIXES.get_or_init(|| {
        let mut p: Vec<(PrefixBox, IriBox)> = Vec::new();
        for (prefix, iri) in config().namespaces.iter() {
            p.push((PrefixBox::new_unchecked(prefix.to_owned().into_boxed_str()), IriBox::new_unchecked(iri.to_owned().into_boxed_str())));
        }
        p.push((PrefixBox::new_unchecked(config().prefix.clone().into_boxed_str()), IriBox::new_unchecked(config().namespace.clone().into_boxed_str())));
        p
    })
}

/// Maps RDF resource URIs to at most one title each, for example "http://example.com/resource/ExampleResource" -> "example resource".
/// Prioritizes title_properties earlier in the list.
fn titles() -> &'static HashMap<String, String> {
    TITLES.get_or_init(|| {
        // TODO: Use a trie instead of a hash map and measure memory consumption when there is a large enough knowledge bases where it could be worth it.
        // Even better would be &str keys referencing the graph, but that is difficult, see branch reftitles.
        let mut tagged = MultiMap::<String, (String, String)>::new();
        let mut titles = HashMap::<String, String>::new();
        for prop in config().title_properties.iter().rev() {
            let term = RefTerm::new_iri(prop.as_ref()).unwrap();
            for tt in graph().triples_with_p(&term) {
                let t = tt.unwrap();
                if let Literal(lit) = t.o() {
                    let lang = if let Some(lang) = lit.lang() { lang.to_string() } else { "".to_owned() };
                    tagged.insert(lang, (t.s().value().to_string(), lit.txt().to_string()));
                }
            }
        }
        // prioritize language tags listed earlier in config().langs
        let mut tags: Vec<&String> = tagged.keys().collect();
        tags.sort_by_cached_key(|tag| config().langs.iter().position(|x| &x == tag).unwrap_or(1000));
        tags.reverse();
        for tag in tags {
            if let Some(v) = tagged.get_vec(tag) {
                for (uri, title) in v {
                    titles.insert(uri.to_owned(), title.to_owned());
                }
            }
        }
        titles
    })
}

/// Maps RDF resource suffixes to at most one type URI each, for example "ExampleResource" -> "http://example.com/resource/ExampleClass".
/// Prioritizes type_properties earlier in the list.
fn types() -> &'static HashMap<String, String> {
    TYPES.get_or_init(|| {
        let mut types = HashMap::<String, String>::new();
        for prop in config().type_properties.iter().rev() {
            let term = RefTerm::new_iri(prop.as_ref()).unwrap();
            for tt in graph().triples_with_p(&term) {
                let t = tt.unwrap();
                let suffix = t.s().value().replace(&config().namespace, "");
                types.insert(suffix, t.o().value().to_string());
            }
        }
        types
    })
}

// Sophia: "A heavily indexed graph. Fast to query but slow to load, with a relatively high memory footprint.".
// Alternatively, use LightGraph, see <https://docs.rs/sophia/latest/sophia/graph/inmem/type.LightGraph.html>.
/// Contains the knowledge base.
static GRAPH: OnceLock<FastGraph> = OnceLock::new();
static PREFIXES: OnceLock<Vec<(PrefixBox, IriBox)>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one title each.
static TITLES: OnceLock<HashMap<String, String>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one type URI each. Result of [types].
static TYPES: OnceLock<HashMap<String, String>> = OnceLock::new();
static NAMESPACE: OnceLock<Namespace<&'static str>> = OnceLock::new();

fn namespace() -> &'static Namespace<&'static str> { NAMESPACE.get_or_init(|| Namespace::new(config().namespace.as_ref()).unwrap()) }

/// Whether the given resource is in subject or object position.
enum ConnectionType {
    Direct,
    Inverse,
}

#[derive(Debug)]
struct Connection {
    prop: IriBox,
    prop_html: String,
    target_htmls: Vec<String>,
}

/// For a given resource r, get either all direct connections (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn connections(conn_type: &ConnectionType, suffix: &str) -> Result<Vec<Connection>, InvalidIri> {
    let source = Piri::from_suffix(suffix);
    let triples = match conn_type {
        ConnectionType::Direct => graph().triples_with_s(&source.iri),
        ConnectionType::Inverse => graph().triples_with_o(&source.iri),
    };
    let mut map: MultiMap<IriBox, String> = MultiMap::new();
    let mut connections: Vec<Connection> = Vec::new();
    for res in triples {
        let triple = res.unwrap();
        let target_term = match conn_type {
            ConnectionType::Direct => triple.o(),
            ConnectionType::Inverse => triple.s(),
        };
        let target_html = match target_term {
            Literal(lit) => match lit.lang() {
                Some(lang) => {
                    format!("{} @{}", lit.txt(), lang)
                }
                None => {
                    format!(r#"{}<div class="datatype">{}</div>"#, lit.txt(), Piri::from(&lit.dt()).short())
                }
            },
            Iri(tiri) => {
                let piri = Piri::from(tiri);
                let title = if let Some(title) = titles().get(&piri.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { "".to_owned() };
                let target = if piri.to_string().starts_with(&config().namespace) { "" } else { " target='_blank' " };
                format!("<a href='{}'{}>{}{}</a>", piri.root_relative(), target, piri.prefixed_string(false, true), title)
            }
            _ => target_term.value().to_string(), // BNode, Variable
        };
        if let Iri(iri) = triple.p() {
            map.insert(IriBox::new_unchecked(iri.value().into()), target_html);
        }
    }
    for (prop, values) in map.into_iter() {
        connections.push(Connection { prop: prop.to_owned(), prop_html: Piri::new(prop).property_anchor(), target_htmls: values.to_vec() });
    }
    Ok(connections)
}

#[cfg(feature = "rdfxml")]
/// Export all triples (s,p,o) for a given subject s as RDF/XML.
pub fn serialize_rdfxml(suffix: &str) -> String {
    let iri = namespace().get(suffix).unwrap();
    RdfXmlSerializer::new_stringifier().serialize_triples(graph().triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(suffix: &str) -> String {
    let iri = namespace().get(suffix).unwrap();
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map(prefixes().to_vec());
    TurtleSerializer::new_stringifier_with_config(config).serialize_triples(graph().triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(suffix: &str) -> String {
    let iri = namespace().get(suffix).unwrap();
    NtSerializer::new_stringifier().serialize_triples(graph().triples_with_s(&iri)).unwrap().to_string()
}

/// Returns the resource with the given suffix from the configured namespace.
pub fn resource(suffix: &str) -> Result<Resource, InvalidIri> {
    let start = Instant::now();
    let subject = namespace().get(suffix).unwrap();
    let uri = subject.clone().value().to_string();

    let all_directs = connections(&ConnectionType::Direct, suffix)?;
    fn filter(cons: &[Connection], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().filter(|c| key_predicate(&c.prop.value())).map(|c| (c.prop_html.clone(), c.target_htmls.clone())).collect()
    }
    let mut descriptions = filter(&all_directs, |key| config().description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !config().description_properties.contains(key));
    let title = titles().get(&uri).unwrap_or(&suffix.to_owned()).to_string();
    let main_type = types().get(suffix).map(|t| t.to_owned());
    let inverses = filter(&connections(&ConnectionType::Inverse, suffix)?, |_| true);
    if all_directs.is_empty() && inverses.is_empty() {
        let warning = format!("No triples found for {}. Did you configure the namespace correctly?", uri);
        warn!("{warning}");
        descriptions.push(("Warning".to_owned(), vec![warning]));
    }
    Ok(Resource {
        suffix: suffix.to_owned(),
        uri,
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: config().github.as_ref().map(|g| format!("{}/issues/new?title={}", g, suffix)),
        main_type,
        descriptions,
        directs: notdescriptions,
        //inverses: connections(&ConnectionType::Inverse, suffix)?,
        inverses,
    })
}
