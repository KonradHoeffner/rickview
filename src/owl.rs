//! OWL axiom display using horned-owl.
//!
//! This module is only compiled when the `owl` Cargo feature is enabled. It parses the configured
//! knowledge base as an OWL ontology and renders axioms about one resource in OWL 2
//! Manchester syntax.
use crate::config::config;
use crate::rdf::{Piri, kb_reader};
use crate::resource::OntologySection;
use horned_owl::error::HornedError;
use horned_owl::io::omn::writer::AsManchester;
use horned_owl::io::{ParserConfiguration, ParserOutput, RDFParserConfiguration};
use horned_owl::model::*;
use horned_owl::ontology::set::SetOntology;
use log::{info, warn};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, Cursor};
use std::path::Path;
use std::sync::OnceLock;

static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");

type Sections = BTreeMap<String, BTreeSet<String>>;

struct OwlIndex {
    components: Vec<AnnotatedComponent<ArcStr>>,
    by_iri: HashMap<String, Vec<usize>>,
}

static INDEX: OnceLock<Option<OwlIndex>> = OnceLock::new();

fn index() -> Option<&'static OwlIndex> { INDEX.get_or_init(load_index).as_ref() }

fn load_index() -> Option<OwlIndex> {
    if config().large {
        info!("OWL axioms are not shown for large knowledge bases.");
        return None;
    }
    let source = config().kb_file.as_deref();
    let format = match source {
        Some(path) if Path::new(path).is_dir() => {
            info!("OWL axioms are not shown for directories, only for a single ontology document.");
            return None;
        }
        Some(path) => {
            let extension = Path::new(path).extension().and_then(std::ffi::OsStr::to_str).unwrap_or("rdf").to_ascii_lowercase();
            match extension.as_str() {
                "ttl" => oxrdfio::RdfFormat::Turtle,
                "nt" => oxrdfio::RdfFormat::NTriples,
                "rdf" | "owl" => oxrdfio::RdfFormat::RdfXml,
                _ => {
                    info!("OWL axioms are not shown for '{path}', only for .ttl, .nt, .rdf or .owl documents.");
                    return None;
                }
            }
        }
        None => oxrdfio::RdfFormat::Turtle,
    };
    let ontology = match load_ontology(source, format) {
        Ok(ontology) => ontology,
        Err(error) => {
            warn!("Could not load OWL ontology from {}: {error}", source.unwrap_or("the example knowledge base"));
            return None;
        }
    };
    let components: Vec<_> = ontology.iter().cloned().collect();
    let mut by_iri = HashMap::<String, Vec<usize>>::new();
    for (i, component) in components.iter().enumerate() {
        for iri in component_iris(&component.component) {
            by_iri.entry(iri).or_default().push(i);
        }
    }
    info!("Loaded {} OWL axioms from {}", components.len(), source.unwrap_or("the example knowledge base"));
    Some(OwlIndex { components, by_iri })
}

fn parser_config() -> ParserConfiguration<ArcStr> {
    let mut config = ParserConfiguration::new(Build::new_arc());
    config.lax = true;
    config
}

fn load_ontology(source: Option<&str>, format: oxrdfio::RdfFormat) -> Result<SetOntology<ArcStr>, HornedError> {
    match source {
        None => parse_rdf(&mut Cursor::new(EXAMPLE_KB), format),
        Some(source) => parse_rdf(&mut kb_reader(source).map_err(|e| HornedError::CommandError(e.to_string()))?, format),
    }
}

fn parse_rdf<R: BufRead>(reader: &mut R, format: oxrdfio::RdfFormat) -> Result<SetOntology<ArcStr>, HornedError> {
    let config = RDFParserConfiguration { common: parser_config(), format: Some(format) };
    let (output, incomplete) = horned_owl::io::rdf::reader::read(reader, config)?;
    let (ontology, ..) = ParserOutput::<ArcStr, ArcAnnotatedComponent>::rdf((output, incomplete)).decompose();
    Ok(ontology)
}

/// All IRIs mentioned by a component, used to decide on which resource pages an axiom is shown.
fn component_iris(component: &Component<ArcStr>) -> BTreeSet<String> {
    component
        .as_manchester()
        .to_string()
        .split('<')
        .skip(1)
        .filter_map(|part| part.split_once('>'))
        .map(|(iri, _)| iri)
        .filter(|iri| is_linkable(iri))
        .map(str::to_owned)
        .collect()
}

pub fn sections(iri: &str) -> Vec<OntologySection> {
    const ORDER: &[&str] = &[
        "Types", "Sub Class Of", "Super Class Of", "Equivalent To", "Disjoint With", "Disjoint Union Of", "Domain", "Range", "In Domain Of",
        "In Range Of", "Characteristics", "Sub Property Of", "Super Property Of", "Inverse Of", "Has Key", "Key For", "Same As", "Different From",
        "Instances", "Rules", "Referenced In",
    ];
    let Some(index) = index() else { return Vec::new() };
    let Some(indices) = index.by_iri.get(iri) else { return Vec::new() };
    let mut sections = Sections::new();
    for i in indices {
        let component = &index.components[*i].component;
        let mut component_sections = BTreeMap::new();
        classify(component, iri, &mut component_sections);
        if component_sections.is_empty() && !is_ignored(component) {
            push(&mut component_sections, "Referenced In", render(component));
        }
        for (label, entries) in component_sections {
            sections.entry(label).or_default().extend(entries);
        }
    }
    let section =
        |label: &str, entries: BTreeSet<String>| OntologySection { label: format!("{label} ({})", entries.len()), entries: entries.into_iter().collect() };
    let mut result: Vec<_> = ORDER.iter().filter_map(|label| sections.remove(*label).map(|entries| section(label, entries))).collect();
    result.extend(sections.into_iter().map(|(label, entries)| section(&label, entries)));
    result
}

fn push(sections: &mut Sections, label: &str, entry: String) {
    if !entry.is_empty() {
        sections.entry(label.to_owned()).or_default().insert(entry);
    }
}

fn named_iri(ce: &ClassExpression<ArcStr>) -> Option<&str> { if let ClassExpression::Class(class) = ce { Some(class.0.as_ref()) } else { None } }

fn ope_iri(ope: &ObjectPropertyExpression<ArcStr>) -> Option<&str> {
    match ope {
        ObjectPropertyExpression::ObjectProperty(property) => Some(property.0.as_ref()),
        ObjectPropertyExpression::InverseObjectProperty(_) => None,
    }
}

fn individual_iri(individual: &Individual<ArcStr>) -> Option<&str> {
    match individual {
        Individual::Named(individual) => Some(individual.0.as_ref()),
        Individual::Anonymous(_) => None,
    }
}

fn property_iri(property: &PropertyExpression<ArcStr>) -> Option<&str> {
    match property {
        PropertyExpression::ObjectPropertyExpression(ope) => ope_iri(ope),
        PropertyExpression::DataProperty(dp) => Some(dp.0.as_ref()),
        PropertyExpression::AnnotationProperty(ap) => Some(ap.0.as_ref()),
    }
}

fn is_linkable(iri: &str) -> bool { iri.starts_with("http://") || iri.starts_with("https://") || iri.starts_with("urn:") }

fn render_iri(iri: &str) -> String { Piri::from(iri).link() }

fn render<T: AsManchester<ArcStr>>(value: &T) -> String
where for<'a> horned_owl::io::omn::Manchester<'a, T, ArcStr>: std::fmt::Display {
    linkify(&value.as_manchester().to_string())
}

fn render_property(property: &PropertyExpression<ArcStr>) -> String {
    match property {
        PropertyExpression::ObjectPropertyExpression(ope) => render(ope),
        PropertyExpression::DataProperty(dp) => render(&dp.0),
        PropertyExpression::AnnotationProperty(ap) => render(&ap.0),
    }
}

fn render_sub_property(sub: &SubObjectPropertyExpression<ArcStr>) -> String {
    match sub {
        SubObjectPropertyExpression::ObjectPropertyExpression(ope) => render(ope),
        SubObjectPropertyExpression::ObjectPropertyChain(chain) => chain.iter().map(render).collect::<Vec<_>>().join(" o "),
    }
}

/// Push `value` under `label` when `condition` is the resource IRI.
fn relate<T: AsManchester<ArcStr>>(condition: Option<&str>, value: &T, iri: &str, label: &str, sections: &mut Sections)
where for<'a> horned_owl::io::omn::Manchester<'a, T, ArcStr>: std::fmt::Display {
    if condition == Some(iri) {
        push(sections, label, render(value));
    }
}

/// Push all members other than the resource IRI under `label`, e.g. equivalent classes.
fn relations<T: AsManchester<ArcStr>>(items: &[T], iri: &str, iri_of: impl Fn(&T) -> Option<&str>, label: &str, sections: &mut Sections)
where for<'a> horned_owl::io::omn::Manchester<'a, T, ArcStr>: std::fmt::Display {
    if items.iter().any(|item| iri_of(item) == Some(iri)) {
        for item in items.iter().filter(|item| iri_of(item) != Some(iri)) {
            push(sections, label, render(item));
        }
    }
}

/// Components that are already visible as ordinary RDF triples and therefore get no OWL section.
fn is_ignored(component: &Component<ArcStr>) -> bool {
    matches!(
        component,
        Component::AnnotationAssertion(_)
            | Component::OntologyAnnotation(_)
            | Component::OntologyID { .. }
            | Component::DocIRI(_)
            | Component::Import(_)
            | Component::DeclareClass(_)
            | Component::DeclareObjectProperty(_)
            | Component::DeclareAnnotationProperty(_)
            | Component::DeclareDataProperty(_)
            | Component::DeclareNamedIndividual(_)
            | Component::DeclareDatatype(_)
            | Component::ObjectPropertyAssertion(_)
            | Component::NegativeObjectPropertyAssertion(_)
            | Component::DataPropertyAssertion(_)
            | Component::NegativeDataPropertyAssertion(_)
            | Component::SubAnnotationPropertyOf(_)
            | Component::AnnotationPropertyDomain(_)
            | Component::AnnotationPropertyRange(_)
    )
}

fn classify(component: &Component<ArcStr>, iri: &str, sections: &mut Sections) {
    let characteristic = match component {
        Component::FunctionalObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Functional"),
        Component::FunctionalDataProperty(dp) if dp.0.as_ref() == iri => Some("Functional"),
        Component::InverseFunctionalObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Inverse Functional"),
        Component::ReflexiveObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Reflexive"),
        Component::IrreflexiveObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Irreflexive"),
        Component::SymmetricObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Symmetric"),
        Component::AsymmetricObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Asymmetric"),
        Component::TransitiveObjectProperty(ope) if ope_iri(&ope.0) == Some(iri) => Some("Transitive"),
        _ => None,
    };
    if let Some(characteristic) = characteristic {
        push(sections, "Characteristics", characteristic.to_owned());
        return;
    }
    match component {
        Component::SubClassOf(SubClassOf { sup, sub }) => {
            relate(named_iri(sub), sup, iri, "Sub Class Of", sections);
            relate(named_iri(sup), sub, iri, "Super Class Of", sections);
        }
        Component::EquivalentClasses(classes) => relations(&classes.0, iri, named_iri, "Equivalent To", sections),
        Component::DisjointClasses(classes) => relations(&classes.0, iri, named_iri, "Disjoint With", sections),
        Component::DisjointUnion(DisjointUnion(class, classes)) => {
            if class.0.as_ref() == iri {
                push(sections, "Disjoint Union Of", classes.iter().map(render).collect::<Vec<_>>().join(" or "));
            }
        }
        Component::ClassAssertion(ClassAssertion { ce, i }) => {
            relate(individual_iri(i), ce, iri, "Types", sections);
            relate(named_iri(ce), i, iri, "Instances", sections);
        }
        Component::ObjectPropertyDomain(ObjectPropertyDomain { ope, ce }) => {
            relate(ope_iri(ope), ce, iri, "Domain", sections);
            relate(named_iri(ce), ope, iri, "In Domain Of", sections);
        }
        Component::ObjectPropertyRange(ObjectPropertyRange { ope, ce }) => {
            relate(ope_iri(ope), ce, iri, "Range", sections);
            relate(named_iri(ce), ope, iri, "In Range Of", sections);
        }
        Component::SubObjectPropertyOf(SubObjectPropertyOf { sup, sub }) => {
            let sub_iri = match sub {
                SubObjectPropertyExpression::ObjectPropertyExpression(ope) => ope_iri(ope),
                SubObjectPropertyExpression::ObjectPropertyChain(_) => None,
            };
            relate(sub_iri, sup, iri, "Sub Property Of", sections);
            if ope_iri(sup) == Some(iri) {
                push(sections, "Super Property Of", render_sub_property(sub));
            }
        }
        Component::EquivalentObjectProperties(properties) => relations(&properties.0, iri, ope_iri, "Equivalent To", sections),
        Component::DisjointObjectProperties(properties) => relations(&properties.0, iri, ope_iri, "Disjoint With", sections),
        Component::InverseObjectProperties(InverseObjectProperties(first, second)) => {
            relate(Some(first.0.as_ref()), second, iri, "Inverse Of", sections);
            relate(Some(second.0.as_ref()), first, iri, "Inverse Of", sections);
        }
        Component::SubDataPropertyOf(SubDataPropertyOf { sup, sub }) => {
            relate(Some(sub.0.as_ref()), sup, iri, "Sub Property Of", sections);
            relate(Some(sup.0.as_ref()), sub, iri, "Super Property Of", sections);
        }
        Component::EquivalentDataProperties(properties) => relations(&properties.0, iri, |property| Some(property.0.as_ref()), "Equivalent To", sections),
        Component::DisjointDataProperties(properties) => relations(&properties.0, iri, |property| Some(property.0.as_ref()), "Disjoint With", sections),
        Component::DataPropertyDomain(DataPropertyDomain { dp, ce }) => {
            relate(Some(dp.0.as_ref()), ce, iri, "Domain", sections);
            relate(named_iri(ce), dp, iri, "In Domain Of", sections);
        }
        Component::DataPropertyRange(DataPropertyRange { dp, dr }) => relate(Some(dp.0.as_ref()), dr, iri, "Range", sections),
        Component::DatatypeDefinition(DatatypeDefinition { kind, range }) => relate(Some(kind.0.as_ref()), range, iri, "Equivalent To", sections),
        Component::HasKey(HasKey { ce, vpe }) => {
            if named_iri(ce) == Some(iri) {
                push(sections, "Has Key", vpe.iter().map(render_property).collect::<Vec<_>>().join(", "));
            }
            if vpe.iter().any(|property| property_iri(property) == Some(iri)) {
                push(sections, "Key For", render(ce));
            }
        }
        Component::SameIndividual(individuals) => relations(&individuals.0, iri, individual_iri, "Same As", sections),
        Component::DifferentIndividuals(individuals) => relations(&individuals.0, iri, individual_iri, "Different From", sections),
        Component::Rule { .. } => push(sections, "Rules", render(component)),
        // Annotation assertions, declarations, imports, ontology metadata and plain property assertions are already visible as normal RDF triples.
        _ => {}
    }
}

fn html_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

/// Escape text and turn `<IRI>` tokens into links.
fn linkify(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        result.push_str(&html_escape(&rest[..start]));
        let after = &rest[start + 1..];
        let Some(end) = after.find('>') else {
            result.push_str("&lt;");
            rest = after;
            continue;
        };
        let iri = &after[..end];
        if is_linkable(iri) {
            result.push_str(&render_iri(iri));
            rest = &after[end + 1..];
        } else {
            result.push_str("&lt;");
            rest = after;
        }
    }
    result.push_str(&html_escape(rest));
    result
}
