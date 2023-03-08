//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::{config::config, resource::Resource};
#[cfg(feature = "hdt")]
use hdt::HdtGraph;
use log::*;
use multimap::MultiMap;
use sophia::api::graph::Graph;
use sophia::api::ns::Namespace;
use sophia::api::prefix::{Prefix, PrefixMap};
use sophia::api::prelude::Triple;
use sophia::api::prelude::TripleSource;
use sophia::api::serializer::{Stringifier, TripleSerializer};
use sophia::api::term::matcher::Any;
use sophia::api::term::SimpleTerm;
use sophia::api::term::Term;
use sophia::api::MownStr;
use sophia::inmem::graph::FastGraph;
use sophia::iri::InvalidIri;
use sophia::iri::Iri;
use sophia::iri::IriRef;
use sophia::turtle::parser::{nt, turtle};
use sophia::turtle::serializer::{nt::NtSerializer, turtle::TurtleConfig, turtle::TurtleSerializer};
#[cfg(feature = "rdfxml")]
use sophia::xml::{self, serializer::RdfXmlSerializer};
use std::{
    collections::BTreeMap, collections::BTreeSet, collections::HashMap, error::Error, fmt, fs::File, io::BufReader, path::Path, sync::OnceLock,
    time::Instant,
};
#[cfg(feature = "hdt")]
use zstd::stream::read::Decoder;

static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");
static CAP: usize = 100; // maximum number of values shown per property

type PrefixItem = (Prefix<Box<str>>, Iri<Box<str>>);

// Prefixed IRI
struct Piri {
    full: String,
    iri: Iri<String>,
    prefixed: Option<(String, String)>,
    //prefixed: Option<(Prefix<&'a str>, String)>,
}

impl fmt::Display for Piri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.full) }
}

impl Piri {
    fn from_suffix(suffix: &str) -> Self { Piri::new(Iri::new_unchecked(config().namespace.to_string() + suffix)) }
    fn new(iri: Iri<String>) -> Self {
        Self { prefixed: prefixes().get_prefixed_pair(iri.clone()).map(|(p, ms)| (p.to_string(), String::from(ms))), full: iri.as_str().to_owned(), iri }
    }
    fn embrace(&self) -> String { format!("&lt;{self}&gt;") }
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

    fn root_relative(&self) -> String { self.full.replace(&*config().namespace, &(config().base.clone() + "/")) }
    fn property_anchor(&self) -> String { format!("<a href='{}'>{}</a>", self.root_relative(), self.prefixed_string(true, false)) }
}

impl From<IriRef<MownStr<'_>>> for Piri {
    fn from(iref: IriRef<MownStr<'_>>) -> Piri { Piri::new(Iri::new_unchecked(iref.as_str().to_owned())) }
}

// Graph cannot be made into a trait object as of Rust 1.67 and Sophia 0.7, see https://github.com/pchampin/sophia_rs/issues/122.
// Enum is cumbersome but we don't have a choice.
// There may be a more elegant way in future Rust and Sophia versions.
#[allow(clippy::large_enum_variant)]
pub enum GraphEnum {
    FastGraph(FastGraph),
    #[cfg(feature = "hdt")]
    HdtGraph(HdtGraph),
}

pub fn kb_reader(filename: &str) -> Result<BufReader<impl std::io::Read>, Box<dyn std::error::Error>> {
    let reader = if filename.starts_with("http") { ureq::get(filename).call()?.into_reader() } else { Box::new(File::open(filename)?) };
    Ok(BufReader::new(reader))
}

/// Load RDF graph from the RDF Turtle file specified in the config.
pub fn graph() -> &'static GraphEnum {
    GRAPH.get_or_init(|| {
        let t = Instant::now();
        let triples = match &config().kb_file {
            None => {
                warn!("No knowledge base configured. Loading example knowledge base. Set kb_file in data/config.toml or env var RICKVIEW_KB_FILE.");
                turtle::parse_str(EXAMPLE_KB).collect_triples()
            }
            Some(filename) => match kb_reader(filename) {
                Err(e) => {
                    error!("Cannot open knowledge base '{}': {}. Check kb_file in data/config.toml or env var RICKVIEW_KB_FILE.", filename, e);
                    std::process::exit(1);
                }
                Ok(br) => {
                    let br = BufReader::new(br);
                    let triples = match Path::new(&filename).extension().and_then(std::ffi::OsStr::to_str) {
                        Some("ttl") => turtle::parse_bufread(br).collect_triples(),
                        Some("nt") => nt::parse_bufread(br).collect_triples(),
                        // error types not compatible
                        #[cfg(feature = "rdfxml")]
                        Some("rdf" | "owl") => Ok(xml::parser::parse_bufread(br).collect_triples().expect("Error parsing {filename} as RDF/XML.")),
                        #[cfg(feature = "hdt")]
                        Some("zst") if filename.ends_with("hdt.zst") => {
                            let decoder = Decoder::with_buffer(br).expect("Error creating zstd decoder.");
                            let hdt = hdt::Hdt::new(BufReader::new(decoder)).expect("Error loading HDT.");
                            info!("Decompressed and loaded HDT from {filename} in {:?}", t.elapsed());
                            return GraphEnum::HdtGraph(hdt::HdtGraph::new(hdt));
                        }
                        #[cfg(feature = "hdt")]
                        Some("hdt") => {
                            let hdt_graph = hdt::HdtGraph::new(hdt::Hdt::new(br).unwrap_or_else(|e| panic!("Error loading HDT from {filename}: {e}")));
                            info!("Loaded HDT from {filename} in {:?}", t.elapsed());
                            return GraphEnum::HdtGraph(hdt_graph);
                        }
                        Some(ext) => {
                            error!("Unknown extension: \"{ext}\": cannot parse knowledge base. Aborting.");
                            std::process::exit(1);
                        }
                        None => {
                            error!("No extension in parse knowledge base file {filename}. Aborting.");
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
            info!(
                "Loaded ~{} FastGraph triples from {} in {:?}",
                g.triples().size_hint().0,
                &config().kb_file.as_deref().unwrap_or("example kb"),
                t.elapsed()
            );
        }
        GraphEnum::FastGraph(g)
    })
}

/// (prefix,iri) pairs from the config
fn prefixes() -> &'static Vec<PrefixItem> {
    PREFIXES.get_or_init(|| {
        let mut p: Vec<PrefixItem> = Vec::new();
        for (prefix, iri) in &config().namespaces {
            p.push((Prefix::new_unchecked(prefix.clone()), Iri::new_unchecked(iri.clone())));
        }
        p.push((Prefix::new_unchecked(config().prefix.clone()), Iri::new_unchecked(config().namespace.clone())));
        p
    })
}

/// Maps RDF resource URIs to at most one title each, for example `http://example.com/resource/ExampleResource` -> "example resource".
/// Prioritizes `title_properties` earlier in the list.
pub fn titles() -> &'static HashMap<String, String> {
    // Code duplication due to Rusts type system, Sophia Graph cannot be used as a trait object.
    match graph() {
        GraphEnum::FastGraph(g) => titles_generic(g),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => titles_generic(g),
    }
}
/// Helper function for [titles].
fn titles_generic<G: Graph>(g: &G) -> &'static HashMap<String, String> {
    TITLES.get_or_init(|| {
        // tag, uri, title
        let mut tagged = MultiMap::<String, (String, String)>::new();
        let mut titles = HashMap::<String, String>::new();
        if !config().large {
            for prop in config().title_properties.iter().rev() {
                match IriRef::new(prop.clone().into()) {
                    Err(_) => {
                        error!("Skipping invalid title property {prop}");
                    }
                    Ok(iref) => {
                        let term = SimpleTerm::Iri(iref);
                        for tt in g.triples_matching(Any, Some(term), Any) {
                            let t = tt.expect("error fetching title triple");
                            let uri = t.s().as_simple().iri().expect("invalid title subject IRI").as_str().to_owned();
                            match t.o().as_simple() {
                                SimpleTerm::LiteralLanguage(lit, tag) => tagged.insert(tag.as_str().to_owned(), (uri, lit.to_string())),
                                SimpleTerm::LiteralDatatype(lit, _) => tagged.insert(String::new(), (uri, lit.to_string())),
                                _ => warn!("Invalid title value {:?}, skipping", t.o().as_simple()),
                            };
                        }
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
                        titles.insert(uri.clone(), title.clone());
                    }
                }
            }
        }
        titles
    })
}

/// Maps RDF resource suffixes to at most one type URI each, for example "`ExampleResource`" -> `http://example.com/resource/ExampleClass`.
/// Prioritizes `type_properties` earlier in the list.
pub fn types() -> &'static HashMap<String, String> {
    // Code duplication due to Rusts type system, Sophia Graph cannot be used as a trait object.
    match graph() {
        GraphEnum::FastGraph(g) => types_generic(g),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => types_generic(g),
    }
}
/// Helper function for [types].
fn types_generic<G: Graph>(g: &G) -> &'static HashMap<String, String> {
    TYPES.get_or_init(|| {
        let mut types = HashMap::<String, String>::new();
        if !config().large {
            for prop in config().type_properties.iter().rev() {
                let iref = IriRef::new(prop.clone().into());
                if iref.is_err() {
                    error!("invalid type property {prop}");
                    continue;
                }
                let term = SimpleTerm::Iri(iref.unwrap());
                for tt in g.triples_matching(Any, Some(term), Any) {
                    let t = tt.expect("error fetching type triple");
                    if !t.s().is_iri() {
                        continue;
                    }
                    let suffix = t.s().as_simple().iri().expect("invalid type subject IRI").to_string().replace(&*config().namespace, "");
                    match t.o().as_simple() {
                        SimpleTerm::Iri(iri) => {
                            types.insert(suffix, iri.to_string());
                        }
                        _ => {
                            warn!("Skipping invalid type {:?} for suffix {suffix} with property <{prop}>.", t.o().as_simple());
                        }
                    };
                }
            }
        }
        types
    })
}

// Sophia: "A heavily indexed graph. Fast to query but slow to load, with a relatively high memory footprint.".
// Alternatively, use LightGraph, see <https://docs.rs/sophia/latest/sophia/graph/inmem/type.LightGraph.html>.
/// Contains the knowledge base.
static GRAPH: OnceLock<GraphEnum> = OnceLock::new();
static PREFIXES: OnceLock<Vec<PrefixItem>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one title each.
static TITLES: OnceLock<HashMap<String, String>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one type URI each. Result of [types].
static TYPES: OnceLock<HashMap<String, String>> = OnceLock::new();
static NAMESPACE: OnceLock<Namespace<&'static str>> = OnceLock::new();

fn namespace() -> &'static Namespace<&'static str> { NAMESPACE.get_or_init(|| Namespace::new(config().namespace.as_ref()).expect("namespace error")) }

/// Whether the given resource is in subject or object position.
enum ConnectionType {
    Direct,
    Inverse,
}

#[derive(Debug)]
struct Connection {
    prop: String,
    prop_html: String,
    target_htmls: Vec<String>,
}

/// For a given resource r, get either all direct connections (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn connections(conn_type: &ConnectionType, suffix: &str) -> Vec<Connection> {
    match graph() {
        GraphEnum::FastGraph(g) => connections_generic(g, conn_type, suffix),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => connections_generic(g, conn_type, suffix),
    }
}

/// Helper function for [connections].
fn connections_generic<G: Graph>(g: &G, conn_type: &ConnectionType, suffix: &str) -> Vec<Connection> {
    let source = Piri::from_suffix(suffix);
    let triples = match conn_type {
        ConnectionType::Direct => g.triples_matching(Some(source.iri), Any, Any),
        ConnectionType::Inverse => g.triples_matching(Any, Any, Some(source.iri)),
    };
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut connections: Vec<Connection> = Vec::new();
    for res in triples {
        let triple = res.expect("error with connection triple");
        let target_term = match conn_type {
            ConnectionType::Direct => triple.o(),
            ConnectionType::Inverse => triple.s(),
        };
        let target_html = match target_term.as_simple() {
            SimpleTerm::LiteralLanguage(lit, tag) => format!("{lit} @{}", tag.as_str()),

            SimpleTerm::LiteralDatatype(lit, dt) => format!(r#"{lit}<div class="datatype">{}</div>"#, Piri::from(dt).short()),

            SimpleTerm::Iri(iri) => {
                let piri = Piri::from(iri);
                let title = if let Some(title) = titles().get(&piri.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { String::new() };
                let target = if piri.to_string().starts_with(&*config().namespace) { "" } else { " target='_blank' " };
                format!("<a href='{}'{target}>{}{title}</a>", piri.root_relative(), piri.prefixed_string(false, true))
            }
            SimpleTerm::BlankNode(blank) => "_:".to_owned() + blank.as_str(),
            _ => format!("{target_term:?}"), // Variable, Triple, ?
        };
        if let SimpleTerm::Iri(iri) = triple.p().as_simple() {
            if let Some(values) = map.get_mut(iri.as_str()) {
                values.insert(target_html);
            } else {
                let mut values = BTreeSet::new();
                values.insert(target_html);
                map.insert(iri.as_str().to_owned(), values);
            }
        };
    }
    for (prop, values) in map {
        let len = values.len();
        let mut target_htmls: Vec<String> = values.into_iter().take(CAP).collect();
        if len > CAP {
            target_htmls.push("...".to_string());
        }
        connections.push(Connection { prop: prop.as_str().to_owned(), prop_html: Piri::new(Iri::new_unchecked(prop)).property_anchor(), target_htmls });
    }
    connections
}

#[cfg(feature = "rdfxml")]
/// Export all triples (s,p,o) for a given subject s as RDF/XML.
pub fn serialize_rdfxml(suffix: &str) -> Result<String, Box<dyn Error>> {
    match graph() {
        GraphEnum::FastGraph(g) => serialize_rdfxml_generic(g, suffix),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => serialize_rdfxml_generic(g, suffix),
    }
}
#[cfg(feature = "rdfxml")]
pub fn serialize_rdfxml_generic<G: Graph>(g: &G, suffix: &str) -> Result<String, Box<dyn Error>> {
    let iri = namespace().get(suffix)?;
    Ok(RdfXmlSerializer::new_stringifier().serialize_triples(g.triples_matching(Some(iri), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(suffix: &str) -> Result<String, Box<dyn Error>> {
    match graph() {
        GraphEnum::FastGraph(g) => serialize_turtle_generic(g, suffix),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => serialize_turtle_generic(g, suffix),
    }
}
fn serialize_turtle_generic<G: Graph>(g: &G, suffix: &str) -> Result<String, Box<dyn Error>> {
    let iri = namespace().get(suffix)?;
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map(prefixes().clone());
    Ok(TurtleSerializer::new_stringifier_with_config(config).serialize_triples(g.triples_matching(Some(iri), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(suffix: &str) -> Result<String, Box<dyn Error>> {
    match graph() {
        GraphEnum::FastGraph(g) => serialize_nt_generic(g, suffix),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => serialize_nt_generic(g, suffix),
    }
}
/// Helper function for [`serialize_nt`].
fn serialize_nt_generic<G: Graph>(g: &G, suffix: &str) -> Result<String, Box<dyn Error>> {
    let iri = namespace().get(suffix)?;
    Ok(NtSerializer::new_stringifier().serialize_triples(g.triples_matching(Some(iri), Any, Any))?.to_string())
}

fn depiction_iri(suffix: &str) -> Option<String> {
    match graph() {
        GraphEnum::FastGraph(g) => depiction_iri_generic(g, suffix),
        #[cfg(feature = "hdt")]
        GraphEnum::HdtGraph(g) => depiction_iri_generic(g, suffix),
    }
}

fn depiction_iri_generic<G: Graph>(g: &G, suffix: &str) -> Option<String> {
    let subject = namespace().get(suffix).ok()?;
    let foaf_depiction = IriRef::new_unchecked("http://xmlns.com/foaf/0.1/depiction");
    g.triples_matching(Some(subject), Some(foaf_depiction), Any)
        .filter_map(Result::ok)
        .map(Triple::to_o)
        .filter(Term::is_iri)
        .map(|o| o.iri().unwrap().as_str().to_owned())
        .next()
}

/// Returns the resource with the given suffix from the configured namespace.
pub fn resource(suffix: &str) -> Result<Resource, InvalidIri> {
    fn filter(cons: &[Connection], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().filter(|c| key_predicate(&c.prop)).map(|c| (c.prop_html.clone(), c.target_htmls.clone())).collect()
    }
    let suffix = str::replace(suffix, " ", "%20");
    let start = Instant::now();
    let subject = namespace().get(&suffix)?;
    let uri = subject.iriref().as_str().to_owned();

    let all_directs = connections(&ConnectionType::Direct, &suffix);
    let descriptions = filter(&all_directs, |key| config().description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !config().description_properties.contains(key));
    let title = titles().get(&uri).unwrap_or(&suffix.clone()).to_string();
    let main_type = types().get(&suffix).map(std::clone::Clone::clone);
    let inverses = if config().show_inverse { filter(&connections(&ConnectionType::Inverse, &suffix), |_| true) } else { Vec::new() };
    /*
    if all_directs.is_empty() && inverses.is_empty() {
        let warning = format!("No triples found for {uri}. Did you configure the namespace correctly?");
        warn!("{warning}");
        descriptions.push(("Warning".to_owned(), vec![warning]));
    }
    */
    Ok(Resource {
        suffix: suffix.clone(),
        uri,
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: config().github.as_ref().map(|g| format!("{g}/issues/new?title={suffix}")),
        main_type,
        descriptions,
        directs: notdescriptions,
        inverses,
        depiction: depiction_iri(&suffix),
    })
}
