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
            None => term.to_string(),
        };
        return s;
}

fn add_prefix(prefixes: &Vec<(PrefixBox,IriBox)>, prefix : &str, iri: &str)
{
    let hito_prefix: PrefixBox = Prefix::new("hito").unwrap().boxed();
    let hito_iri: IriBox = Iri::new("http://hitontology.eu/ontology/").unwrap().boxed();
    let hito_ns = Namespace::new("http://hitontology.eu/ontology/").unwrap();
    prefixes.push((hito_prefix,hito_iri));

}

fn main() {
    let file = File::open("hito.nt").expect("Unable to open file");
    let reader = BufReader::new(file);
    let graph: FastGraph = turtle::parse_bufread(reader).collect_triples().unwrap();
    
    let mut prefixes: Vec<(PrefixBox,IriBox)> = Vec::new();
    let hito_prefix: PrefixBox = Prefix::new("hito").unwrap().boxed();
    let hito_iri: IriBox = Iri::new("http://hitontology.eu/ontology/").unwrap().boxed();
    let hito_ns = Namespace::new("http://hitontology.eu/ontology/").unwrap();
    prefixes.push((hito_prefix,hito_iri));

    let subject = hito_ns.get("SoftwareProduct").unwrap();

    println!("<html><body>");
    println!("<h1>{}</h1>",subject);
    let results = graph.triples_with_s(&subject);
     print!("<ul>");
    for res in results {
        let t = res.unwrap();
         println!("<li>{} {}</li>",prefix_term(&prefixes,t.p()),prefix_term(&prefixes,t.o()));
    }
    print!("</ul>");
    /*
    println!("Inverse");
    let results = graph.triples_with_o(&subject);
    for res in results {
        let t = res.unwrap();
    println!("is {} of {}",t.s(),t.p());
    }
    */
    println!("</body></html>");
}
