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
    term::{RefTerm, TTerm, Term::*},
    triple::{stream::TripleSource, Triple},
};
use std::{collections::HashMap, fs::File, io::BufReader, time::Instant};

/// If the namespace is known, returns a prefixed term string, for example "rdfs:label".
/// Otherwise, returns the full IRI.
fn prefix_iri(prefixes: &Vec<(PrefixBox, IriBox)>, iri: &Iri) -> String {
    let suffix = prefixes.get_prefixed_pair(iri);
    match suffix {
        //Some(x) => format!("{}:<b>{}</b>",x.0.to_string(),&x.1),
        Some(x) => format!("{}:{}", x.0.to_string(), &x.1),
        None => iri.value().to_string(),
    }
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

/// Maps RDF resource suffixes to at most one title each, for example "ExampleResource" -> "example resource".
/// Prioritizes title_properties earlier in the list.
/// Language tags are not yet used.
fn titles() -> HashMap<String, String> {
    let mut titles = HashMap::<String, String>::new();
    for prop in CONFIG.title_properties.iter().rev() {
        let term = RefTerm::new_iri(prop.as_ref()).unwrap();
        //print!("{}",term);
        for tt in GRAPH.triples_with_p(&term) {
            let t = tt.unwrap();
            let suffix = t.s().value().replace(&CONFIG.namespace, "");
            titles.insert(suffix, t.o().value().to_string());
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
    static ref HITO_NS: Namespace<&'static str> = Namespace::new(CONFIG.namespace.as_ref()).unwrap();
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

fn property_anchor(iri: &Iri) -> String {
    let root_relative = iri.value().to_string().replace(&CONFIG.namespace, &("/".to_owned() + &CONFIG.base_path));
    format!("<a href='{}'>{}</a>", root_relative, prefix_iri(&PREFIXES, iri))
}

#[derive(Debug)]
struct Connection {
    prop: IriBox,
    prop_html: String,
    to_htmls: Vec<String>,
}

/// For a given resource r, get either all direct connections (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn connections(conn_type: &ConnectionType, suffix: &str) -> Result<Vec<Connection>, InvalidIri> {
    let mut iri = HITO_NS.get(suffix)?;
    // Sophia bug workaround when suffix is empty, see https://github.com/pchampin/sophia_rs/issues/115
    if suffix.is_empty() {
        iri = sophia::term::SimpleIri::new(&CONFIG.namespace, std::option::Option::None).unwrap();
    }
    let triples = match conn_type {
        ConnectionType::Direct => GRAPH.triples_with_s(&iri),
        ConnectionType::Inverse => GRAPH.triples_with_o(&iri),
    };
    let mut map: MultiMap<IriBox, String> = MultiMap::new();
    let mut connections: Vec<Connection> = Vec::new();
    //let mut to_htmls: Vec<String> = Vec::new();
    for res in triples {
        let triple = res.unwrap();
        let to_term = match conn_type {
            ConnectionType::Direct => triple.o(),
            ConnectionType::Inverse => triple.s(),
        };
        let to_html = match to_term {
            Literal(lit) => match lit.lang() {
                Some(lang) => {
                    format!("{} @{}", lit.txt(), lang)
                }
                None => {
                    format!(r#"{}<div class="datatype">{}</div>"#, lit.txt(), &prefix_iri(&PREFIXES, &Iri::new_unchecked(&lit.dt().value())))
                }
            },
            Iri(iri) => {
                let full = &iri.value().to_string();
                let suffix = &iri.normalized_suffixed_at_last_gen_delim().suffix().to_owned().unwrap().to_string();
                let prefixed = prefix_iri(&PREFIXES, &Iri::new_unchecked(&iri.value()));
                let root_relative = full.replace(&CONFIG.namespace, &("/".to_owned() + &CONFIG.base_path));
                let title = if let Some(title) = TITLES.get(suffix) { format!("<br><span>&#8618; {title}</span>") } else { "".to_owned() };
                format!("<a href='{}'>{prefixed}{}</a>", root_relative, title)
            }
            _ => to_term.value().to_string(), // BNode, Variable
        };
        if let Iri(iri) = triple.p() {
            map.insert(IriBox::new_unchecked(iri.value().into()), to_html);
        }
    }
    for (prop, values) in map.iter_all() {
        connections.push(Connection { prop: prop.to_owned(), prop_html: property_anchor(&prop.as_iri()), to_htmls: values.to_vec() });
    }
    Ok(connections)
}

#[cfg(feature = "rdfxml")]
/// Export all triples (s,p,o) for a given subject s as RDF/XML.
pub fn serialize_rdfxml(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    RdfXmlSerializer::new_stringifier().serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map((PREFIXES).to_vec());
    TurtleSerializer::new_stringifier_with_config(config).serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    NtSerializer::new_stringifier().serialize_triples(GRAPH.triples_with_s(&iri)).unwrap().to_string()
}

/// Returns the resource with the given suffix from the configured namespace.
pub fn resource(suffix: &str) -> Result<Resource, InvalidIri> {
    let start = Instant::now();
    let subject = HITO_NS.get(suffix).unwrap();

    let all_directs = connections(&ConnectionType::Direct, suffix)?;
    fn filter(cons: &[Connection], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().filter(|c| key_predicate(&c.prop.value())).map(|c| (c.prop_html.clone(), c.to_htmls.clone())).collect()
    }
    let descriptions = filter(&all_directs, |key| CONFIG.description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !CONFIG.description_properties.contains(key));
    let title = TITLES.get(suffix).unwrap_or(&suffix.to_owned()).to_string();
    let main_type = TYPES.get(suffix).map(|t| t.to_owned());
    Ok(Resource {
        suffix: suffix.to_owned(),
        uri: subject.clone().value().to_string(),
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
