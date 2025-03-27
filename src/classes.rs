use crate::rdf::{Piri, graph, titles};
use multimap::MultiMap;
use sophia::api::MownStr;
use sophia::api::term::IriRef;
use sophia::api::term::SimpleTerm::Iri;
use sophia::api::term::matcher::Any;
use std::collections::HashSet;

type IriM<'a> = IriRef<MownStr<'a>>;

fn node(class: &IriM<'_>, subclasses: &MultiMap<&IriM<'_>, &IriM<'_>>) -> (String, u32) {
    let piri = Piri::from(class);
    let title = if let Some(title) = titles().get(&piri.to_string()) { title } else { &piri.short() };
    let mut inner = String::new();
    let mut count = 0;
    let s = match subclasses.get_vec(class) {
        Some(children) => {
            for child in children {
                let (child_s, child_count) = &node(child, subclasses);
                inner += child_s;
                count += child_count + 1;
            }
            format!(
                "<details style='margin: 1em;'><summary><a href='{}' target='_blank'>{} ({count})</summary>{inner}</details>\n",
                piri.root_relative(),
                title
            )
        }
        None => format!("<p style='margin: 1em;'>&bull; <a href='{}' target='_blank'>{}</p>", piri.root_relative(), title),
    };
    (s, count)
}

pub fn class_tree() -> String {
    let g = graph();
    // the graphs we use should never fail
    // rdfs:subclassOf is also used with blank nodes for owl restrictions that we ignore
    let pairs: Vec<[IriM<'_>; 2]> = g
        .triples_matching(Any, Some(sophia::api::ns::rdfs::subClassOf), Any)
        .map(|t| t.expect("error fetching class triple terms"))
        .filter_map(|t| match t {
            [Iri(child), _, Iri(parent)] => Some([child, parent]),
            _ => None,
        })
        .collect();
    let mut subclasses = MultiMap::<&IriM<'_>, &IriM<'_>>::new();
    let mut children = HashSet::<&IriM<'_>>::new();
    for [child, parent] in &pairs {
        subclasses.insert(parent, child);
        children.insert(child);
    }
    let parents: HashSet<_> = subclasses.keys().copied().collect();
    let roots = parents.difference(&children);

    let mut s = String::new();
    s += "<html><body>";
    for root in roots {
        s += &node(root, &subclasses).0;
    }
    s += "<body></html>";
    s
}
