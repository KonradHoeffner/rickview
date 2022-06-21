use crate::config::CONFIG;
use crate::resource::Resource;
use multimap::MultiMap;
use sophia::graph::{inmem::sync::FastGraph, *};
use sophia::iri::IriBox;
use sophia::ns::Namespace;
use sophia::parser::turtle;
use sophia::prefix::{PrefixBox, PrefixMap};
use sophia::term::Term;
use sophia::triple::stream::TripleSource;
use sophia::triple::Triple;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Instant;

fn prefix_term(prefixes: &Vec<(PrefixBox, IriBox)>, term: &Term<Arc<str>>) -> String {
    let suffix = prefixes.get_prefixed_pair(term);
    let s = match suffix {
        Some(x) => x.0.to_string() + ":" + &x.1.to_string(),
        None => term.to_string().replace(['<', '>'], ""),
    };
    return s;
}

fn load_graph() -> FastGraph {
    let file = File::open(&CONFIG.kb_file).expect(&format!(
        "Unable to open knowledge base file '{}'. Make sure that the file exists. You may be able to download it with the prepare script. Configure as kb_file in data/config.toml or using the environment variable RICKVIEW_KB_FILE.",
        &CONFIG.kb_file
    ));
    let reader = BufReader::new(file);
    turtle::parse_bufread(reader).collect_triples().unwrap()
}

fn prefixes() -> Vec<(PrefixBox, IriBox)> {
    let mut p: Vec<(PrefixBox, IriBox)> = Vec::new();
    for (prefix, iri) in CONFIG.namespaces.iter() {
        p.push((
            PrefixBox::new_unchecked(prefix.to_owned().into_boxed_str()),
            IriBox::new_unchecked(iri.to_owned().into_boxed_str()),
        ));
    }
    p
}

lazy_static! {
    static ref PREFIXES: Vec<(PrefixBox, IriBox)> = prefixes();
    static ref GRAPH: FastGraph = load_graph();
    static ref HITO_NS: Namespace<&'static str> =
        Namespace::new(CONFIG.namespace.as_ref()).unwrap();
}

enum ConnectionType {
    DIRECT,
    INVERSE,
}

fn linker(object: &String) -> String {
    if object.starts_with('"') {
        return object.replace('"', "").to_owned();
    }
    let suffix = object.replace("hito:", "");
    return format!("<a href='{suffix}'>{object}</a>");
}

fn connections(tt: &ConnectionType, suffix: &str) -> Vec<(String, Vec<String>)> {
    let mut map: MultiMap<String, String> = MultiMap::new();
    let iri = HITO_NS.get(suffix).unwrap();
    let results = match tt {
        ConnectionType::DIRECT => GRAPH.triples_with_s(&iri),
        ConnectionType::INVERSE => GRAPH.triples_with_o(&iri),
    };
    let mut d: Vec<(String, Vec<String>)> = Vec::new();
    for res in results {
        let t = res.unwrap();
        map.insert(
            prefix_term(&PREFIXES, t.p()),
            prefix_term(
                &PREFIXES,
                match tt {
                    ConnectionType::DIRECT => t.o(),
                    ConnectionType::INVERSE => t.s(),
                },
            ),
        );
    }
    for (key, values) in map.iter_all() {
        d.push((key.to_owned(), values.iter().map(linker).collect()));
    }
    d
}

pub fn resource(suffix: &str) -> Resource {
    let start = Instant::now();
    let subject = HITO_NS.get(suffix).unwrap();

    let uri = subject.to_string().replace(['<', '>'], "");
    let all_directs = connections(&ConnectionType::DIRECT, suffix);
    let descriptions = all_directs
        .iter()
        .cloned()
        .filter(|c| CONFIG.description_properties.contains(&c.0))
        .collect();
    let notdescriptions = all_directs
        .into_iter()
        .filter(|c| !CONFIG.description_properties.contains(&c.0))
        .collect();
    Resource {
        suffix: suffix.to_owned(),
        uri,
        duration: format!("{:?}", start.elapsed()),
        descriptions,
        directs: notdescriptions,
        inverses: connections(&ConnectionType::INVERSE, suffix),
    }
}
