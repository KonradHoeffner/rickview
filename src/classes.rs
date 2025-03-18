use crate::rdf::graph;
//use sophia::api::prelude::{Term, Triple};
use sophia::api::term::SimpleTerm::{self, Iri};
use sophia::api::term::matcher::Any;
use std::collections::{HashMap, HashSet};

fn node(class: &SimpleTerm<'_>, subclasses: &HashMap<SimpleTerm<'_>, SimpleTerm<'_>>) -> (String, u32) {
    let mut uri = String::new();
    let mut title = String::new();
    match class {
        Iri(iri) => {
            //let siri = iri.as_str().to_owned();
            uri += iri.as_str();
            title += iri.as_str();
        }
        _ => {
            //return format!("<span style='color:red;'>Invalid term: {class:?} is not a class</span>");
            return (String::new(), 0);
        }
    }
    let mut inner = String::new();
    let mut count = 0;
    for (child, _) in subclasses.iter().filter(|(_, p)| *p == class) {
        let (child_s, child_count) = &node(child, subclasses);
        inner += child_s;
        count += 1;
        count += child_count;
    }
    match count {
        0 => (format!("<p style='margin-left:2em;'>&bull; <a href='{uri}' target='_blank'>{title}</p>"), 0),
        _ => (format!("<details style='margin-left:2em;'><summary><a href='{uri}' target='_blank'>{title} ({count})</summary>{inner}</details>\n"), count),
    }
}

pub fn class_tree() -> String {
    let g = graph();
    let mut subclasses = HashMap::<SimpleTerm<'_>, SimpleTerm<'_>>::new();
    for tt in g.triples_matching(Any, Some(sophia::api::ns::rdfs::subClassOf), Any) {
        let t: [SimpleTerm<'_>; 3] = tt.expect("error fetching class triple");
        let [child, _, parent] = t;
        subclasses.insert(child, parent);
    }
    let parents: HashSet<_> = subclasses.values().collect();
    let children: HashSet<_> = subclasses.keys().collect();
    //let binding = HashSet::<&SimpleTerm<'_>>::from_iter(subclasses.keys());
    //let binding = HashSet::<&SimpleTerm<'_>>::from_iter(subclasses.keys());
    let roots = parents.difference(&children);

    let mut s = String::new();
    s += "<html><body>";
    for root in roots {
        s += &node(root, &subclasses).0;
    }
    s += "<body></html>";
    s
}
