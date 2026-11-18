use crate::rdf::{Piri, graph, titles};
use multimap::MultiMap;
use sophia::api::MownStr;
use sophia::api::ns::{owl, rdf, rdfs};
use sophia::api::term::IriRef;
use sophia::api::term::SimpleTerm::Iri;
use sophia::api::term::matcher::Any;
use std::collections::HashSet;

type IriM<'a> = IriRef<MownStr<'a>>;

fn escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

fn node(class: &IriM<'_>, subclasses: &MultiMap<&IriM<'_>, &IriM<'_>>, path: &mut HashSet<String>) -> Option<(String, u32)> {
    if !path.insert(class.to_string()) {
        return None;
    }
    let piri = Piri::from(class);
    let title = titles().get(&piri.to_string()).map_or_else(|| piri.short(), Clone::clone);
    let mut inner = String::new();
    let mut count = 0;
    if let Some(children) = subclasses.get_vec(class) {
        for child in children {
            if let Some((child_s, child_count)) = node(child, subclasses, path) {
                inner += &child_s;
                count += child_count + 1;
            }
        }
    }
    path.remove(&class.to_string());
    let link = format!("<a href='{}'>{}</a>", piri.root_relative(), escape(&title));
    Some(if inner.is_empty() {
        (format!("<li>{link}</li>"), 0)
    } else {
        (format!("<li><details><summary>{link} ({count})</summary><ul>{inner}</ul></details></li>"), count)
    })
}

pub fn class_tree() -> String {
    let g = graph();
    // rdfs:subClassOf is also used with blank nodes for OWL restrictions, which are ignored.
    let pairs: Vec<[IriM<'_>; 2]> = g
        .triples_matching(Any, Some(rdfs::subClassOf), Any)
        .map(|t| t.expect("error fetching class triple terms"))
        .filter_map(|t| match t {
            [Iri(child), _, Iri(parent)] => Some([child, parent]),
            _ => None,
        })
        .collect();
    let declared: Vec<IriM<'_>> = g
        .triples_matching(Any, Some(rdf::type_), Some(owl::Class))
        .map(|t| t.expect("error fetching class declaration"))
        .filter_map(|t| match t {
            [Iri(class), _, _] => Some(class),
            _ => None,
        })
        .collect();
    let mut subclasses = MultiMap::<&IriM<'_>, &IriM<'_>>::new();
    let mut children = HashSet::<&IriM<'_>>::new();
    let mut nodes = HashSet::<&IriM<'_>>::new();
    for class in &declared {
        nodes.insert(class);
    }
    for [child, parent] in &pairs {
        subclasses.insert(parent, child);
        children.insert(child);
        nodes.insert(child);
        nodes.insert(parent);
    }
    let mut roots: Vec<_> = nodes.difference(&children).copied().collect();
    roots.sort_by_cached_key(|root| {
        let piri = Piri::from(*root);
        titles().get(&piri.to_string()).map_or_else(|| piri.short(), Clone::clone).to_lowercase()
    });

    let mut s = String::from("<ul class='class-tree'>");
    for root in roots {
        if let Some((html, _)) = node(root, &subclasses, &mut HashSet::new()) {
            s += &html;
        }
    }
    s += "</ul>";
    s
}
