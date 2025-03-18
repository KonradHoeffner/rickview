use crate::rdf::{Piri, graph, titles};
use sophia::api::term::SimpleTerm::{self, Iri};
use sophia::api::term::matcher::Any;
use std::collections::{HashMap, HashSet};

fn node(class: &SimpleTerm<'_>, subclasses: &HashMap<SimpleTerm<'_>, SimpleTerm<'_>>) -> (String, u32) {
    match class {
        Iri(iri) => {
            let piri = Piri::from(iri);
            let title = if let Some(title) = titles().get(&piri.to_string()) { title } else { &piri.short() };
            let mut inner = String::new();
            let mut count = 0;
            for (child, _) in subclasses.iter().filter(|(_, p)| *p == class) {
                let (child_s, child_count) = &node(child, subclasses);
                inner += child_s;
                count += 1;
                count += child_count;
            }
            match count {
                0 => (format!("<p style='margin: 1em;'>&bull; <a href='{}' target='_blank'>{}</p>", piri.root_relative(), title), 0),
                _ => (
                    format!(
                        "<details style='margin: 1em;'><summary><a href='{}' target='_blank'>{} ({count})</summary>{inner}</details>\n",
                        piri.root_relative(),
                        title
                    ),
                    count,
                ),
            }
        }
        _ => {
            //format!("<span style='color:red;'>Invalid term: {class:?} is not a class</span>")
            (String::new(), 0)
        }
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
    let roots = parents.difference(&children);

    let mut s = String::new();
    s += "<html><body>";
    for root in roots {
        s += &node(root, &subclasses).0;
    }
    s += "<body></html>";
    s
}
