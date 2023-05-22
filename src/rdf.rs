//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::config::config;
use crate::resource::Resource;
#[cfg(feature = "hdt")]
use hdt::HdtGraph;
use log::*;
use multimap::MultiMap;
use sophia::api::graph::Graph;
use sophia::api::prefix::{Prefix, PrefixMap};
use sophia::api::prelude::{Triple, TripleSource};
use sophia::api::serializer::{Stringifier, TripleSerializer};
use sophia::api::term::bnode_id::BnodeId;
use sophia::api::term::matcher::{Any, TermMatcher};
use sophia::api::term::{FromTerm, SimpleTerm, Term};
use sophia::inmem::graph::FastGraph;
use sophia::iri::{Iri, IriRef};
use sophia::turtle::parser::{nt, turtle};
use sophia::turtle::serializer::nt::NtSerializer;
use sophia::turtle::serializer::turtle::{TurtleConfig, TurtleSerializer};
#[cfg(feature = "rdfxml")]
use sophia::xml::{self, serializer::RdfXmlSerializer};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;
#[cfg(feature = "hdt")]
use zstd::stream::read::Decoder;

const BLANK_RECURSIVE: bool = false;
static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");
static CAP: usize = 100; // maximum number of values shown per property
static SKOLEM_START: &str = ".well-known/genid/";

type PrefixItem = (Prefix<Box<str>>, Iri<Box<str>>);

// Prefixed IRI
struct Piri {
    full: String,
    prefixed: Option<(String, String)>,
}

impl fmt::Display for Piri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.full) }
}

impl Piri {
    fn new(iri: Iri<&str>) -> Self {
        Self { prefixed: prefixes().get_prefixed_pair(iri).map(|(p, ms)| (p.to_string(), String::from(ms))), full: iri.as_str().to_owned() }
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
    fn suffix(&self) -> String { self.prefixed.as_ref().map_or_else(|| self.full.clone(), |pair| pair.1.clone()) }
    fn root_relative(&self) -> String { self.full.replace(config().namespace.as_str(), &(config().base.clone() + "/")) }
    fn property_anchor(&self) -> String { format!("<a href='{}'>{}</a>", self.root_relative(), self.prefixed_string(true, false)) }
}

impl From<IriRef<&str>> for Piri {
    fn from(iref: IriRef<&str>) -> Piri { Piri::new(Iri::new_unchecked(&iref)) }
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

impl GraphEnum {
    fn triples_matching<'s, S, P, O>(&'s self, sm: S, pm: P, om: O) -> Box<dyn Iterator<Item = Result<[SimpleTerm<'static>; 3], Infallible>> + '_>
    where
        S: TermMatcher + 's,
        P: TermMatcher + 's,
        O: TermMatcher + 's,
    {
        match self {
            // both graphs produce infallible results
            GraphEnum::FastGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
            #[cfg(feature = "hdt")]
            GraphEnum::HdtGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
        }
    }
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
        p.push((Prefix::new_unchecked(config().prefix.clone()), config().namespace.clone()));
        p
    })
}

/// Maps RDF resource URIs to at most one title each, for example `http://example.com/resource/ExampleResource` -> "example resource".
/// Prioritizes `title_properties` earlier in the list.
pub fn titles() -> &'static HashMap<String, String> {
    TITLES.get_or_init(|| {
        let g = graph();
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
                            // ignore blank node labels as title because they usually don't have any
                            if t.s().is_blank_node() {
                                continue;
                            }
                            let uri = t.s().as_simple().iri().expect("invalid title subject IRI").as_str().to_owned();
                            match t.o() {
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
                for tt in graph().triples_matching(Any, Some(term), Any) {
                    let t = tt.expect("error fetching type triple");
                    if !t.s().is_iri() {
                        continue;
                    }
                    let suffix = t.s().as_simple().iri().expect("invalid type subject IRI").to_string().replace(config().namespace.as_str(), "");
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

impl From<Connection> for (String, Vec<String>) {
    fn from(c: Connection) -> Self { (c.prop_html, c.target_htmls) }
}

/// Map skolemized IRIs back to blank nodes. Keep deskolemized IRIs as they are.
fn deskolemize<'a>(iri: &'a Iri<&str>) -> SimpleTerm<'a> {
    if let Some(id) = iri.as_str().split(SKOLEM_START).nth(1) {
        SimpleTerm::from_term(BnodeId::new_unchecked(id.to_owned()))
    } else {
        iri.as_simple()
    }
}

/// For a given resource r, get either all direct connections (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn connections(conn_type: &ConnectionType, source: &SimpleTerm<'_>) -> Vec<Connection> {
    let g = graph();
    let triples = match conn_type {
        ConnectionType::Direct => g.triples_matching(Some(source), Any, Any),
        ConnectionType::Inverse => g.triples_matching(Any, Any, Some(source)),
    };
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut conns: Vec<Connection> = Vec::new();
    for res in triples {
        let triple = res.expect("error with connection triple");
        let target_term = match conn_type {
            ConnectionType::Direct => triple.o(),
            ConnectionType::Inverse => triple.s(),
        };
        let target_html = match target_term.as_simple() {
            SimpleTerm::LiteralLanguage(lit, tag) => format!("{lit} @{}", tag.as_str()),

            SimpleTerm::LiteralDatatype(lit, dt) => format!(r#"{lit}<div class="datatype">{}</div>"#, Piri::from(dt.as_ref()).short()),

            SimpleTerm::Iri(iri) => {
                let piri = Piri::from(iri.as_ref());
                let title = if let Some(title) = titles().get(&piri.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { String::new() };
                let target = if piri.to_string().starts_with(config().namespace.as_str()) { "" } else { " target='_blank' " };
                format!("<a href='{}'{target}>{}{title}</a>", piri.root_relative(), piri.prefixed_string(false, true))
            }
            // https://www.w3.org/TR/rdf11-concepts/ Section 3.5 Replacing Blank Nodes with IRIs
            SimpleTerm::BlankNode(blank) => {
                let id = blank.as_str();
                // prevent infinite loop
                let sub_html = if BLANK_RECURSIVE && matches!(conn_type, ConnectionType::Direct) && !source.is_blank_node() {
                    connections(&ConnectionType::Direct, target_term)
                        .into_iter()
                        .map(|c| c.target_htmls.iter().map(|html| c.prop_html.clone() + " " + html + "").collect::<Vec<_>>().join("<br>"))
                        .collect::<Vec<_>>()
                        .join("<br>")
                } else {
                    String::new()
                };
                let r = IriRef::new_unchecked(SKOLEM_START.to_owned() + id);
                let iri = config().namespace.resolve(r);
                format!("<a href='{}'>_:{id}</a><br>{sub_html}", Piri::new(iri.as_ref()).root_relative())
            }
            _ => format!("{target_term:?}"),
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
        conns.push(Connection { prop: prop.as_str().to_owned(), prop_html: Piri::new(Iri::new_unchecked(&prop)).property_anchor(), target_htmls });
    }
    conns
}

#[cfg(feature = "rdfxml")]
/// Export all triples (s,p,o) for a given subject s as RDF/XML.
pub fn serialize_rdfxml(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    Ok(RdfXmlSerializer::new_stringifier().serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map(prefixes().clone());
    Ok(TurtleSerializer::new_stringifier_with_config(config).serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    Ok(NtSerializer::new_stringifier().serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

fn depiction_iri(iri: Iri<&str>) -> Option<String> {
    let foaf_depiction = IriRef::new_unchecked("http://xmlns.com/foaf/0.1/depiction");
    graph()
        .triples_matching(Some(iri), Some(foaf_depiction), Any)
        .filter_map(Result::ok)
        .map(Triple::to_o)
        .filter(Term::is_iri)
        .map(|o| o.iri().unwrap().as_str().to_owned())
        .next()
}

/// Returns the resource with the given IRI from the configured namespace.
pub fn resource(subject: Iri<&str>) -> Resource {
    fn filter(cons: &[Connection], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().filter(|c| key_predicate(&c.prop)).map(|c| (c.prop_html.clone(), c.target_htmls.clone())).collect()
    }
    let convert = |v: Vec<Connection>| v.into_iter().map(Connection::into).collect();
    let start = Instant::now();
    let piri = Piri::new(subject.as_ref());
    let suffix = piri.suffix();

    let source = deskolemize(&subject);
    let all_directs = connections(&ConnectionType::Direct, &source);
    let descriptions = filter(&all_directs, |key| config().description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !config().description_properties.contains(key));
    let title = titles().get(&piri.full).unwrap_or(&suffix).to_string().replace(SKOLEM_START, "Blank Node ");
    let main_type = types().get(&suffix).map(std::clone::Clone::clone);
    let inverses = if config().show_inverse { convert(connections(&ConnectionType::Inverse, &source)) } else { Vec::new() };
    Resource {
        uri: piri.full,
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: config().github.as_ref().map(|g| format!("{g}/issues/new?title={suffix}")),
        main_type,
        descriptions,
        directs: notdescriptions,
        inverses,
        depiction: depiction_iri(subject),
    }
}
