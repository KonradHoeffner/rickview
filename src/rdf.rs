use crate::config::CONFIG;
use crate::resource::Resource;
use multimap::MultiMap;
use sophia::parser::turtle;
use sophia::prefix::{PrefixBox, PrefixMap};
#[cfg(feature = "rdfxml")]
use sophia::serializer::xml::RdfXmlSerializer;
use sophia::serializer::{
    nt::NtSerializer,
    turtle::{TurtleConfig, TurtleSerializer},
    Stringifier, TripleSerializer,
};
use sophia::term::{RefTerm, TTerm, Term};
use sophia::triple::{stream::TripleSource, Triple};
use sophia::{
    graph::{inmem::sync::FastGraph, *},
    iri::IriBox,
    ns::Namespace,
};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Instant;

// if the namespace is known, returns a prefixed term string, for example "rdfs:label"
// otherwise, returns the full IRI
fn prefix_term(prefixes: &Vec<(PrefixBox, IriBox)>, term: &Term<Arc<str>>) -> String {
    let suffix = prefixes.get_prefixed_pair(term);
    match suffix {
        Some(x) => x.0.to_string() + ":" + &x.1.to_string(),
        None => term.to_string().replace(['<', '>'], ""),
    }
}

fn load_graph() -> FastGraph {
    let file = File::open(&CONFIG.kb_file).unwrap_or_else(|_| panic!(
        "Unable to open knowledge base file '{}'. Make sure that the file exists. You may be able to download it with the prepare script. Configure as kb_file in data/config.toml or using the environment variable RICKVIEW_KB_FILE.",
        &CONFIG.kb_file
    ));
    let reader = BufReader::new(file);
    let graph: FastGraph = turtle::parse_bufread(reader)
        .collect_triples()
        .unwrap_or_else(|_| panic!("Unable to parse knowledge base file {}", &CONFIG.kb_file));
    log::debug!("{} triples loaded from {}", graph.triples().count(), &CONFIG.kb_file);
    graph
}

// (prefix,iri) pairs from the config
fn prefixes() -> Vec<(PrefixBox, IriBox)> {
    let mut p: Vec<(PrefixBox, IriBox)> = Vec::new();
    for (prefix, iri) in CONFIG.namespaces.iter() {
        p.push((
            PrefixBox::new_unchecked(prefix.to_owned().into_boxed_str()),
            IriBox::new_unchecked(iri.to_owned().into_boxed_str()),
        ));
    }
    p.push((
        PrefixBox::new_unchecked(CONFIG.prefix.clone().into_boxed_str()),
        IriBox::new_unchecked(CONFIG.namespace.clone().into_boxed_str()),
    ));
    p
}

// prioritizes title properties earlier in the list
// language tags are not yet used
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

// prioritizes type properties earlier in the list
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

enum ConnectionType {
    Direct,
    Inverse,
}

fn linker(object: &String) -> String {
    if object.starts_with('"') {
        return object.replace('"', "");
    }
    let suffix = object.replace(&format!("{}:", &CONFIG.prefix), "");
    return format!(
        "<a href='{suffix}'>{object}</a><br><span>&#8618; {}</span>",
        TITLES.get(&suffix).unwrap_or(&suffix)
    );
}

fn connections(tt: &ConnectionType, suffix: &str) -> Vec<(String, Vec<String>)> {
    let mut map: MultiMap<String, String> = MultiMap::new();
    let iri = HITO_NS.get(suffix).unwrap();
    let results = match tt {
        ConnectionType::Direct => GRAPH.triples_with_s(&iri),
        ConnectionType::Inverse => GRAPH.triples_with_o(&iri),
    };
    let mut d: Vec<(String, Vec<String>)> = Vec::new();
    for res in results {
        let t = res.unwrap();
        map.insert(
            prefix_term(&PREFIXES, t.p()),
            prefix_term(
                &PREFIXES,
                match tt {
                    ConnectionType::Direct => t.o(),
                    ConnectionType::Inverse => t.s(),
                },
            ),
        );
    }
    for (key, values) in map.iter_all() {
        d.push((key.to_owned(), values.iter().map(linker).collect()));
    }
    d
}

/*
pub struct SimpleResource {
    pub suffix: String,
    pub title: String,
}

pub fn simple_resource(suffix: &str) -> SimpleResource {
    let subject = HITO_NS.get(suffix).unwrap();
    let title = (|| -> Result<String, sophia::iri::error::InvalidIri> {
        Ok(GRAPH
            .triples_with_sp(&subject, &RDFS_NS.get("label")?)
            .next()
            .ok_or(sophia::iri::error::InvalidIri)?
            .o()
            .to_string())
    })()
    .unwrap_or(suffix.to_owned());
    SimpleResource {
        suffix: suffix.to_owned(),
        title: title,
    }
}
*/

#[cfg(feature = "rdfxml")]
pub fn serialize_rdfxml(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    RdfXmlSerializer::new_stringifier()
        .serialize_triples(GRAPH.triples_with_s(&iri))
        .unwrap()
        .to_string()
}

pub fn serialize_turtle(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map((&PREFIXES).to_vec());
    TurtleSerializer::new_stringifier_with_config(config)
        .serialize_triples(GRAPH.triples_with_s(&iri))
        .unwrap()
        .to_string()
}

pub fn serialize_nt(suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    NtSerializer::new_stringifier()
        .serialize_triples(GRAPH.triples_with_s(&iri))
        .unwrap()
        .to_string()
}

pub fn resource(suffix: &str) -> Option<Resource> {
    let start = Instant::now();
    let subject = HITO_NS.get(suffix).unwrap();

    let uri = subject.to_string().replace(['<', '>'], "");
    let all_directs = connections(&ConnectionType::Direct, suffix);
    fn filter(cons: &[(String, Vec<String>)], key_predicate: fn(&str) -> bool) -> Vec<(String, Vec<String>)> {
        cons.iter().cloned().filter(|c| key_predicate(&c.0)).collect()
    }
    let descriptions = filter(&all_directs, |key| CONFIG.description_properties.contains(key));
    let notdescriptions = filter(&all_directs, |key| !CONFIG.description_properties.contains(key));
    /*let titles = filter(&all_directs, |key| CONFIG.title_properties.contains(&key.to_string()));
    let title: String = || -> Option<String> {
        Some(
            titles
                .get(0)?
                .1
                .get(0)?
                .to_string()
                .split("@")
                .next()?
                .to_owned(),
        )
    }()*/
    let title = TITLES.get(suffix)?.to_string();
    let main_type = TYPES.get(suffix)?.to_string();
    //.unwrap_or(&suffix.to_owned());
    Some(Resource {
        suffix: suffix.to_owned(),
        uri,
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: CONFIG.github.as_ref().map(|g| format!("{}/issues/new?title={}", g, suffix)),
        main_type,
        descriptions,
        directs: notdescriptions,
        inverses: connections(&ConnectionType::Inverse, suffix),
    })
}
