use sophia::graph::{inmem::FastGraph, *};
use sophia::ns::Namespace;
use sophia::parser::turtle;
use sophia::triple::stream::TripleSource;
use sophia::triple::Triple;
use std::fs::File;
use std::io::BufReader;
use sophia::prefix::{PrefixMap,PrefixBox,Prefix};
use sophia::iri::{Iri,IriBox};
use sophia::term::Term;
use std::rc::Rc;

fn prefix_term(prefixes: &Vec<(PrefixBox,IriBox)>, term: &Term<Rc<str>>) -> String
{
    let suffix = prefixes.get_prefixed_pair(term);
        let s = match suffix
        {
            Some(x) => x.0.to_string() +":"+ &x.1.to_string(),
            None => term.to_string().replace(['<','>'],""),
        };
        return s;
}

fn add_prefix(mut prefixes: Vec<(PrefixBox,IriBox)>, prefix : &str, iri: &str) -> Vec<(PrefixBox,IriBox)>
{
    let prefix_box: PrefixBox = Prefix::new(prefix).unwrap().boxed();
    let iri_box: IriBox = Iri::new(iri).unwrap().boxed();
    prefixes.push((prefix_box,iri_box));
    return prefixes;
}

fn main() {
    let file = File::open("hito.nt").expect("Unable to open file");
    let reader = BufReader::new(file);
    let graph: FastGraph = turtle::parse_bufread(reader).collect_triples().unwrap();
    
    let mut prefixes: Vec<(PrefixBox,IriBox)> = Vec::new();
    prefixes = add_prefix(prefixes,"hito","http://hitontology.eu/ontology/");
    prefixes = add_prefix(prefixes,"purl","http://purl.org/vocab/vann/");
    prefixes = add_prefix(prefixes,"rdf","http://www.w3.org/1999/02/22-rdf-syntax-ns#");
    prefixes = add_prefix(prefixes,"rdfs","http://www.w3.org/2000/01/rdf-schema#");
    prefixes = add_prefix(prefixes,"owl","http://www.w3.org/2002/07/owl#");
    prefixes = add_prefix(prefixes,"skos","http://www.w3.org/2004/02/skos/core#");
    prefixes = add_prefix(prefixes,"skos","http://www.w3.org/2004/02/skos/core#");
    prefixes = add_prefix(prefixes,"sh","http://www.w3.org/ns/shacl#");

    let hito_ns = Namespace::new("http://hitontology.eu/ontology/").unwrap();

    let subject = hito_ns.get("SoftwareProduct").unwrap();

    println!("<html><body>");
    println!("<h1>{}</h1>",subject.to_string().replace(['<','>'],""));
    let results = graph.triples_with_s(&subject);
     print!("<ul>");
    for res in results {
        let t = res.unwrap();
         println!("<li>{} {}</li>",prefix_term(&prefixes,t.p()),prefix_term(&prefixes,t.o()));
    }
    print!("</ul>");
    println!("## Inverse");
    let results = graph.triples_with_o(&subject);
    print!("<ul>");
    for res in results {
        let t = res.unwrap();
         println!("<li>is {} of {}</li>",prefix_term(&prefixes,t.p()),prefix_term(&prefixes,t.s()));
    }
    print!("</ul>");
    println!("</body></html>");
}
