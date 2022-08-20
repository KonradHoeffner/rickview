use crate::{config::CONFIG, resource::Resource};
use multimap::MultiMap;
#[cfg(feature = "rdfxml")]
use sophia::serializer::xml::RdfXmlSerializer;
use sophia::{
    graph::{inmem::sync::FastGraph, *},
    iri::{error::InvalidIri, IriBox},
    ns::Namespace,
    parser::turtle,
    prefix::{PrefixBox, PrefixMap},
    serializer::{
        nt::NtSerializer,
        turtle::{TurtleConfig, TurtleSerializer},
        Stringifier, TripleSerializer,
    },
    term::{RefTerm, TTerm, Term},
    triple::{stream::TripleSource, Triple},
};
use std::{collections::HashMap, fs::File, io::BufReader, sync::Arc, time::Instant};

/// If the namespace is known, returns a prefixed term string, for example "rdfs:label".
/// Otherwise, returns the full IRI.
fn prefix_term(prefixes: &Vec<(PrefixBox, IriBox)>, term: &Term<Arc<str>>) -> String {
    let suffix = prefixes.get_prefixed_pair(term);
    match suffix {
        Some(x) => x.0.to_string() + ":" + &x.1,
        None => term.to_string().replace(['<', '>'], ""),
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

/// Prioritizes title properties earlier in the list.
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

/// Prioritizes type properties earlier in the list.
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
    static ref GRAPH: FastGraph = load_graph();
    static ref HITO_NS: Namespace<&'static str> = Namespace::new(CONFIG.namespace.as_ref()).unwrap();
    static ref RDFS_NS: Namespace<&'static str> = Namespace::new("http://www.w3.org/2000/01/rdf-schema#").unwrap();
    static ref TITLES: HashMap<String, String> = titles();
    static ref TYPES: HashMap<String, String> = types();
}

/// Whether the given resource is in subject or object position.
enum ConnectionType {
    Direct,
    Inverse,
}

/// Generate HTML anchor element for the URI given as (prefixed, full), for example ("ex:Example", "http://example.com/Example").
fn linker((prefixed, full): &(String, String)) -> String {
    if prefixed.starts_with('"') {
        return prefixed.replace('"', "");
    }
    let root_relative = full.replace(&CONFIG.namespace, &("/".to_owned() + &CONFIG.base_path));
    format!("<a href='{}'>{}</a><br><span>&#8618; {}</span>", root_relative, prefixed, TITLES.get(full).unwrap_or(prefixed))
}

fn property_anchor(term: &Term<Arc<str>>) -> String {
    let root_relative = term.to_string().replace(['<', '>'], "").replace(&CONFIG.namespace, &("/".to_owned() + &CONFIG.base_path));
    format!("<a href='{}'>{}</a>", root_relative, prefix_term(&PREFIXES, term))
}

/// For a given resource r, get either all direct connections (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn connections(tt: &ConnectionType, suffix: &str) -> Result<Vec<(String, Vec<String>)>, InvalidIri> {
    let mut iri = HITO_NS.get(suffix)?;
    // Sophia bug workaround when suffix is empty, see https://github.com/pchampin/sophia_rs/issues/115
    if suffix.is_empty() {
        iri = sophia::term::SimpleIri::new(&CONFIG.namespace, std::option::Option::None).unwrap();
    }
    let results = match tt {
        ConnectionType::Direct => GRAPH.triples_with_s(&iri),
        ConnectionType::Inverse => GRAPH.triples_with_o(&iri),
    };
    let mut map: MultiMap<String, (String, String)> = MultiMap::new();
    let mut d: Vec<(String, Vec<String>)> = Vec::new();
    for res in results {
        let t = res.unwrap();
        let right = match tt {
            ConnectionType::Direct => t.o(),
            ConnectionType::Inverse => t.s(),
        };
        map.insert(property_anchor(t.p()), (prefix_term(&PREFIXES, right), right.value().to_string()));
    }
    for (key, values) in map.iter_all() {
        d.push((key.to_owned(), values.iter().map(linker).collect()));
    }
    Ok(d)
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

    let uri = subject.to_string().replace(['<', '>'], "");
    let all_directs = connections(&ConnectionType::Direct, suffix)?;
    fn filter(cons: &[(String, Vec<String>)], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().cloned().filter(|c| key_predicate(&c.0)).collect()
    }
    let descriptions = filter(&all_directs, |key| CONFIG.description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !CONFIG.description_properties.contains(key));
    let title = TITLES.get(suffix).unwrap_or(&suffix.to_owned()).to_string();
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
        inverses: connections(&ConnectionType::Inverse, suffix)?,
    })
}
