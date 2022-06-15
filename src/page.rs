use multimap::MultiMap;
use sophia::graph::{inmem::sync::FastGraph, *};
use sophia::iri::{Iri, IriBox};
use sophia::ns::Namespace;
use sophia::parser::turtle;
use sophia::prefix::{Prefix, PrefixBox, PrefixMap};
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

fn add_prefix(
    mut prefixes: Vec<(PrefixBox, IriBox)>,
    prefix: &str,
    iri: &str,
) -> Vec<(PrefixBox, IriBox)> {
    let prefix_box: PrefixBox = Prefix::new(prefix).unwrap().boxed();
    let iri_box: IriBox = Iri::new(iri).unwrap().boxed();
    prefixes.push((prefix_box, iri_box));
    return prefixes;
}

fn load_graph() -> FastGraph {
    let file = File::open("hito.ttl").expect("Unable to open file");
    let reader = BufReader::new(file);
    turtle::parse_bufread(reader).collect_triples().unwrap()
}

fn prefixes() -> Vec<(PrefixBox, IriBox)> {
    let mut prefixes: Vec<(PrefixBox, IriBox)> = Vec::new();
    prefixes = add_prefix(prefixes, "hito", "http://hitontology.eu/ontology/");
    prefixes = add_prefix(prefixes, "purl", "http://purl.org/vocab/vann/");
    prefixes = add_prefix(
        prefixes,
        "rdf",
        "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
    );
    prefixes = add_prefix(prefixes, "rdfs", "http://www.w3.org/2000/01/rdf-schema#");
    prefixes = add_prefix(prefixes, "owl", "http://www.w3.org/2002/07/owl#");
    prefixes = add_prefix(prefixes, "skos", "http://www.w3.org/2004/02/skos/core#");
    prefixes = add_prefix(prefixes, "skos", "http://www.w3.org/2004/02/skos/core#");
    prefixes = add_prefix(prefixes, "sh", "http://www.w3.org/ns/shacl#");
    prefixes = add_prefix(prefixes, "ov", "http://open.vocab.org/terms/");
    prefixes
}

lazy_static! {
    static ref PREFIXES: Vec<(PrefixBox, IriBox)> = prefixes();
    static ref GRAPH: FastGraph = load_graph();
    static ref HITO_NS: Namespace<&'static str> =
        Namespace::new("http://hitontology.eu/ontology/").unwrap();
}

enum TableType {
    SUBJECT,
    OBJECT,
}

fn table(tt: &TableType, suffix: &str) -> String {
    let iri = HITO_NS.get(suffix).unwrap();
    let mut s = String::new();
    let results = match tt {
        TableType::SUBJECT => GRAPH.triples_with_s(&iri),
        TableType::OBJECT => GRAPH.triples_with_o(&iri),
    };
    s.push_str("<table>\n");
    let mut m: MultiMap<String, String> = MultiMap::new();
    for res in results {
        let t = res.unwrap();
        m.insert(
            prefix_term(&PREFIXES, t.p()),
            prefix_term(
                &PREFIXES,
                match tt {
                    TableType::SUBJECT => t.o(),
                    TableType::OBJECT => t.s(),
                },
            ),
        );
    }
    for (p, os) in m {
        s.push_str(&format!("<tr><td>{p}</td><td><span>"));
        for o in os {
            let link = o.replace("hito:", "");
            s.push_str(&format!("<a href='{link}'>{o}</a><br>"));
        }
        s.push_str("</span></td></tr>");
    }
    s.push_str("</table>");
    s
}

pub fn page(suffix: &str) -> String {
    let start = Instant::now();
    let subject = HITO_NS.get(suffix).unwrap();

    let mut s: String = "<!DOCTYPE html><html><body><head><link rel='stylesheet' href='/rickview.css'></style></head>\n".to_owned();
    s.push_str(&format!(
        "<h1>{}</h1>\n<h2>{}</h2>",
        suffix,
        subject.to_string().replace(['<', '>'], "")
    ));
    s.push_str(&table(&TableType::SUBJECT, &suffix));

    s.push_str("<h3>Inverse</h3>");
    s.push_str(&table(&TableType::OBJECT, &suffix));

    s.push_str(&format!("{:?}", start.elapsed()));
    s.push_str("</body>\n</html>\n");
    s
}
