//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::{config::CONFIG, resource::Resource};
use multimap::MultiMap;
#[cfg(feature = "rdfxml")]
use sophia::serializer::xml::RdfXmlSerializer;
use sophia::{
    graph::{inmem::sync::FastGraph, *},
    iri::{error::InvalidIri, AsIri, Iri, IriBox},
    ns::Namespace,
    parser::turtle,
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
use std::{collections::HashMap, fmt, fs::File, io::BufReader, time::Instant};

fn get_prefixed_pair(iri: &Iri) -> Option<(String, String)> {
    let (p, s) = PREFIXES.get_prefixed_pair(iri)?;
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
    fn from_suffix(suffix: &str) -> Self {
        Piri::new(IriBox::new_unchecked((CONFIG.namespace.clone() + suffix).into_boxed_str()))
     }
    fn new(iri: IriBox) -> Self { Self { prefixed: get_prefixed_pair(&iri.as_iri()), iri } }
    fn embrace(&self) -> String {format!("&lt;{}&gt;",self)}
    fn prefixed_string(&self, bold: bool ,embrace: bool) -> String {
        if let Some((p,s)) = &self.prefixed {
            if bold {format!("{p}:<b>{s}</b>")}  else {format!("{p}:{s}")}
        } else if embrace {self.embrace()} else {self.to_string()}
    }
    fn short(&self) -> String { self.prefixed_string(false,false)}

    fn root_relative(&self) -> String { self.iri.value().replace(&CONFIG.namespace, &(CONFIG.base_path.clone() + "/")) }
    fn property_anchor(&self) -> String { format!("<a href='{}'>{}</a>", self.root_relative(), self.prefixed_string(true,false)) }
}

/// Load RDF graph from the RDF Turtle file specified in the config.
fn load_graph() -> FastGraph {
    match File::open(&CONFIG.kb_file) {
        Err(e) => {
            log::error!("Cannot open knowledge base file '{}': {}. Check kb_file in data/config.toml or env var RICKVIEW_KB_FILE.", &CONFIG.kb_file, e);
            std::process::exit(1);
        }
        Ok(file) => {
            let reader = BufReader::new(file);
            let graph: FastGraph = turtle::parse_bufread(reader).collect_triples().unwrap_or_else(|x| {
                log::error!("Unable to parse knowledge base file {}: {}", &CONFIG.kb_file, x);
                std::process::exit(1);
            });
            if log::log_enabled!(log::Level::Debug) {
                log::debug!("~ {} triples loaded from {}", graph.triples().size_hint().0, &CONFIG.kb_file);
            }
            graph
        }
    }
}

/// (prefix,iri) pairs from the config
fn prefixes() -> Vec<(PrefixBox, IriBox)> {
    let mut p: Vec<(PrefixBox, IriBox)> = Vec::new();
    for (prefix, iri) in CONFIG.namespaces.iter() {
        p.push((PrefixBox::new_unchecked(prefix.to_owned().into_boxed_str()), IriBox::new_unchecked(iri.to_owned().into_boxed_str())));
    }
    p.push((PrefixBox::new_unchecked(CONFIG.prefix.clone().into_boxed_str()), IriBox::new_unchecked(CONFIG.namespace.clone().into_boxed_str())));
    p
}

/// Maps RDF resource URIs to at most one title each, for example "http://example.com/resource/ExampleResource" -> "example resource".
/// Prioritizes title_properties earlier in the list.
/// Language tags are not yet used.
fn titles() -> HashMap<String, String> {
    // TODO: Use a trie instead of a hash map and measure memory consumption when there is a large enough knowledge bases where it could be worth it.
    // Even better would be &str keys referencing the graph, but that is difficult, see branch reftitles.
    let mut titles = HashMap::<String, String>::new();
    for prop in CONFIG.title_properties.iter().rev() {
        let term = RefTerm::new_iri(prop.as_ref()).unwrap();
        for tt in GRAPH.triples_with_p(&term) {
            let t = tt.unwrap();
            let x = t.s().value().to_string();
            if x == "http://www.snik.eu/ontology/meta" {log::info!("{:?} {:?} {:?}",t.s(),t.p(),t.o());}
            titles.insert(t.s().value().to_string(), t.o().value().to_string());
        }
    }
    titles
}

/// Maps RDF resource suffixes to at most one type URI each, for example "ExampleResource" -> "http://example.com/resource/ExampleClass".
/// Prioritizes type_properties earlier in the list.
fn types() -> HashMap<String, String> {
    let mut types = HashMap::<String, String>::new();
    for prop in CONFIG.type_properties.iter().rev() {
        let term = RefTerm::new_iri(prop.as_ref()).unwrap();
        for tt in GRAPH.triples_with_p(&term) {
            let t = tt.unwrap();
            let suffix = t.s().value().replace(&CONFIG.namespace, "");
            types.insert(suffix, t.o().value().to_string());
        }
    }
    types
}

lazy_static! {
    static ref PREFIXES: Vec<(PrefixBox, IriBox)> = prefixes();
    // Sophia: "A heavily indexed graph. Fast to query but slow to load, with a relatively high memory footprint.".
    // Alternatively, use LightGraph, see <https://docs.rs/sophia/latest/sophia/graph/inmem/type.LightGraph.html>.
    /// Contains the knowledge base.
    static ref GRAPH: FastGraph = load_graph();
    static ref NAMESPACE: Namespace<&'static str> = Namespace::new(CONFIG.namespace.as_ref()).unwrap();
    static ref RDFS_NS: Namespace<&'static str> = Namespace::new("http://www.w3.org/2000/01/rdf-schema#").unwrap();
    /// Map of RDF resource suffixes to at most one title each. Result of [titles].
    static ref TITLES: HashMap<String, String> = titles();
    /// Map of RDF resource suffixes to at most one type URI each. Result of [types].
    static ref TYPES: HashMap<String, String> = types();
}

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
        ConnectionType::Direct => GRAPH.triples_with_s(&source.iri),
        ConnectionType::Inverse => GRAPH.triples_with_o(&source.iri),
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
                let title = if let Some(title) = TITLES.get(&piri.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { "".to_owned() };
                let target = if piri.to_string().starts_with(&CONFIG.namespace) { "" } else { " target='_blank' " };
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
    let iri = NAMESPACE.get(suffix).unwrap();
    RdfXmlSerializer::new_stringifier().serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(suffix: &str) -> String {
    let iri = NAMESPACE.get(suffix).unwrap();
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map((PREFIXES).to_vec());
    TurtleSerializer::new_stringifier_with_config(config).serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(suffix: &str) -> String {
    let iri = NAMESPACE.get(suffix).unwrap();
    NtSerializer::new_stringifier().serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Returns the resource with the given suffix from the configured namespace.
pub fn resource(suffix: &str) -> Result<Resource, InvalidIri> {
    let start = Instant::now();
    let subject = NAMESPACE.get(suffix).unwrap();
    let uri = subject.clone().value().to_string();

    let all_directs = connections(&ConnectionType::Direct, suffix)?;
    fn filter(cons: &[Connection], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().filter(|c| key_predicate(&c.prop.value())).map(|c| (c.prop_html.clone(), c.target_htmls.clone())).collect()
    }
    let descriptions = filter(&all_directs, |key| CONFIG.description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !CONFIG.description_properties.contains(key));
    let title = TITLES.get(&uri).unwrap_or(&suffix.to_owned()).to_string();
    let main_type = TYPES.get(suffix).map(|t| t.to_owned());
    Ok(Resource {
        suffix: suffix.to_owned(),
        uri,
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: CONFIG.github.as_ref().map(|g| format!("{}/issues/new?title={}", g, suffix)),
        main_type,
        descriptions,
        directs: notdescriptions,
        //inverses: connections(&ConnectionType::Inverse, suffix)?,
        inverses: filter(&connections(&ConnectionType::Inverse, suffix)?, |_| true),
    })
}
