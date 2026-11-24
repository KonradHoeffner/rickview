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
use horned_owl::io::{InputFormat, ParserConfiguration, ParserOutput, RDFParserConfiguration};
use horned_owl::model::*;
use horned_owl::ontology::set::SetOntology;
use log::{info, warn};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{BufRead, Cursor};
use std::path::Path;
use std::str::FromStr;
use std::sync::OnceLock;

static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");

struct OwlIndex {
    components: Vec<AnnotatedComponent<ArcStr>>,
    by_iri: HashMap<String, Vec<usize>>,
}

static INDEX: OnceLock<Option<OwlIndex>> = OnceLock::new();

fn index() -> Option<&'static OwlIndex> { INDEX.get_or_init(load_index).as_ref() }

fn load_index() -> Option<OwlIndex> {
    if !config().owl || config().large {
        return None;
    }
    let configured = config().kb_file.as_deref();
    let (source, extension) = match configured {
        Some(path) if Path::new(path).is_dir() => {
            info!("OWL axioms are not shown for directories, only for a single ontology document.");
            return None;
        }
        Some(path) => {
            let extension = Path::new(path).extension().and_then(std::ffi::OsStr::to_str).unwrap_or("guess").to_ascii_lowercase();
            if InputFormat::from_str(&extension).is_err() {
                info!("OWL axioms are not shown for '{path}', only for a .ttl, .nt, .rdf, .owl, .ofn, .omn, .owx or .obo document.");
                return None;
            }
            (Some(path), extension)
        }
        None => (None, "ttl".to_owned()),
    };
    let ontology = match load_ontology(source, &extension) {
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

fn load_ontology(source: Option<&str>, extension: &str) -> Result<SetOntology<ArcStr>, HornedError> {
    if source.is_none() {
        let mut reader = Cursor::new(EXAMPLE_KB);
        return parse_rdf(&mut reader, Some(oxrdfio::RdfFormat::Turtle));
    }
    let source = source.expect("source is set");
    let mut reader = kb_reader(source).map_err(|e| HornedError::CommandError(e.to_string()))?;
    match extension {
        "ofn" => {
            let (ontology, _) = horned_owl::io::ofn::reader::read::<ArcStr, _, SetOntology<ArcStr>, _>(&mut reader, parser_config())?;
            Ok(ontology)
        }
        "omn" => {
            let (ontology, _) = horned_owl::io::omn::reader::read::<ArcStr, _, SetOntology<ArcStr>, _>(reader, parser_config())?;
            Ok(ontology)
        }
        "owx" => {
            let (ontology, _) = horned_owl::io::owx::reader::read::<ArcStr, _, SetOntology<ArcStr>, _>(&mut reader, parser_config())?;
            Ok(ontology)
        }
        "obo" => {
            let (ontology, _) = horned_owl::io::obo::reader::read::<ArcStr, _, SetOntology<ArcStr>, _>(&mut reader, parser_config())?;
            Ok(ontology)
        }
        _ => {
            let format = match InputFormat::from_str(extension) {
                Ok(InputFormat::Rdf(format)) => format,
                _ => None,
            };
            parse_rdf(&mut reader, format)
        }
    }
}

fn parse_rdf<R: BufRead>(reader: &mut R, format: Option<oxrdfio::RdfFormat>) -> Result<SetOntology<ArcStr>, HornedError> {
    let config = RDFParserConfiguration { common: parser_config(), format };
    let (output, incomplete) = horned_owl::io::rdf::reader::read(reader, config)?;
    let (ontology, ..) = ParserOutput::<ArcStr, ArcAnnotatedComponent>::rdf((output, incomplete)).decompose();
    Ok(ontology)
}

/// All IRIs mentioned by a component, used to decide on which resource pages an axiom is shown.
fn component_iris(component: &Component<ArcStr>) -> BTreeSet<String> {
    let text = component.as_manchester().to_string();
    let mut iris = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('>') else { break };
        let iri = &rest[..end];
        if iri.starts_with("http://") || iri.starts_with("https://") || iri.starts_with("urn:") {
            iris.insert(iri.to_owned());
        }
        rest = &rest[end + 1..];
    }
    iris
}

pub fn sections(iri: &str) -> Vec<OntologySection> {
    const ORDER: &[&str] = &[
        "Types", "Sub Class Of", "Super Class Of", "Equivalent To", "Disjoint With", "Disjoint Union Of", "Domain", "Range", "In Domain Of",
        "In Range Of", "Characteristics", "Sub Property Of", "Super Property Of", "Inverse Of", "Has Key", "Key For", "Same As", "Different From",
        "Instances", "Rules", "Referenced In",
    ];
    let Some(index) = index() else { return Vec::new() };
    let Some(indices) = index.by_iri.get(iri) else { return Vec::new() };
    let mut sections = BTreeMap::<String, BTreeSet<String>>::new();
    for i in indices {
        let component = &index.components[*i].component;
        let mut component_sections = BTreeMap::new();
        classify(component, iri, &mut component_sections);
        if component_sections.is_empty() && !is_ignored(component) {
            push(&mut component_sections, "Referenced In", render_component(component));
        }
        for (label, entries) in component_sections {
            sections.entry(label).or_default().extend(entries);
        }
    }
    let mut result = Vec::new();
    for label in ORDER {
        if let Some(entries) = sections.remove(*label) {
            result.push(OntologySection { label: (*label).to_owned(), entries: entries.into_iter().collect() });
        }
    }
    for (label, entries) in sections {
        result.push(OntologySection { label, entries: entries.into_iter().collect() });
    }
    result
}

fn push(sections: &mut BTreeMap<String, BTreeSet<String>>, label: &str, entry: String) {
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

fn render_iri(iri: &str) -> String { Piri::from(iri).link() }

fn render_ce(ce: &ClassExpression<ArcStr>) -> String {
    if let ClassExpression::Class(class) = ce {
        return render_iri(class.0.as_ref());
    }
    linkify(&ce.as_manchester().to_string())
}

fn render_dr(dr: &DataRange<ArcStr>) -> String {
    if let DataRange::Datatype(datatype) = dr {
        return render_iri(datatype.0.as_ref());
    }
    linkify(&dr.as_manchester().to_string())
}

fn render_individual(individual: &Individual<ArcStr>) -> String {
    if let Individual::Named(individual) = individual {
        return render_iri(individual.0.as_ref());
    }
    linkify(&individual.as_manchester().to_string())
}

fn render_ope(ope: &ObjectPropertyExpression<ArcStr>) -> String {
    if let ObjectPropertyExpression::ObjectProperty(property) = ope {
        return render_iri(property.0.as_ref());
    }
    linkify(&ope.as_manchester().to_string())
}

fn render_property(property: &PropertyExpression<ArcStr>) -> String {
    match property {
        PropertyExpression::ObjectPropertyExpression(ope) => render_ope(ope),
        PropertyExpression::DataProperty(dp) => render_iri(dp.0.as_ref()),
        PropertyExpression::AnnotationProperty(ap) => render_iri(ap.0.as_ref()),
    }
}

fn render_sub_property(sub: &SubObjectPropertyExpression<ArcStr>) -> String {
    match sub {
        SubObjectPropertyExpression::ObjectPropertyExpression(ope) => render_ope(ope),
        SubObjectPropertyExpression::ObjectPropertyChain(chain) => chain.iter().map(render_ope).collect::<Vec<_>>().join(" o "),
    }
}

fn render_component(component: &Component<ArcStr>) -> String { linkify(&component.as_manchester().to_string()) }

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

#[allow(clippy::too_many_lines)]
fn classify(component: &Component<ArcStr>, iri: &str, sections: &mut BTreeMap<String, BTreeSet<String>>) {
    match component {
        Component::SubClassOf(SubClassOf { sup, sub }) => {
            if named_iri(sub) == Some(iri) {
                push(sections, "Sub Class Of", render_ce(sup));
            } else if named_iri(sup) == Some(iri) {
                push(sections, "Super Class Of", render_ce(sub));
            }
        }
        Component::EquivalentClasses(classes) => {
            if classes.0.iter().any(|ce| named_iri(ce) == Some(iri)) {
                for ce in classes.0.iter().filter(|ce| named_iri(ce) != Some(iri)) {
                    push(sections, "Equivalent To", render_ce(ce));
                }
            }
        }
        Component::DisjointClasses(classes) => {
            if classes.0.iter().any(|ce| named_iri(ce) == Some(iri)) {
                for ce in classes.0.iter().filter(|ce| named_iri(ce) != Some(iri)) {
                    push(sections, "Disjoint With", render_ce(ce));
                }
            }
        }
        Component::DisjointUnion(DisjointUnion(class, classes)) => {
            if class.0.as_ref() == iri {
                push(sections, "Disjoint Union Of", classes.iter().map(render_ce).collect::<Vec<_>>().join(" or "));
            }
        }
        Component::ClassAssertion(ClassAssertion { ce, i }) => {
            if individual_iri(i) == Some(iri) {
                push(sections, "Types", render_ce(ce));
            } else if named_iri(ce) == Some(iri) {
                push(sections, "Instances", render_individual(i));
            }
        }
        Component::ObjectPropertyDomain(ObjectPropertyDomain { ope, ce }) => {
            if ope_iri(ope) == Some(iri) {
                push(sections, "Domain", render_ce(ce));
            } else if named_iri(ce) == Some(iri) {
                push(sections, "In Domain Of", render_ope(ope));
            }
        }
        Component::ObjectPropertyRange(ObjectPropertyRange { ope, ce }) => {
            if ope_iri(ope) == Some(iri) {
                push(sections, "Range", render_ce(ce));
            } else if named_iri(ce) == Some(iri) {
                push(sections, "In Range Of", render_ope(ope));
            }
        }
        Component::FunctionalObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Functional".to_owned());
            }
        }
        Component::InverseFunctionalObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Inverse Functional".to_owned());
            }
        }
        Component::ReflexiveObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Reflexive".to_owned());
            }
        }
        Component::IrreflexiveObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Irreflexive".to_owned());
            }
        }
        Component::SymmetricObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Symmetric".to_owned());
            }
        }
        Component::AsymmetricObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Asymmetric".to_owned());
            }
        }
        Component::TransitiveObjectProperty(ope) => {
            if ope_iri(&ope.0) == Some(iri) {
                push(sections, "Characteristics", "Transitive".to_owned());
            }
        }
        Component::SubObjectPropertyOf(SubObjectPropertyOf { sup, sub }) => {
            if let SubObjectPropertyExpression::ObjectPropertyExpression(sub) = sub
                && ope_iri(sub) == Some(iri)
            {
                push(sections, "Sub Property Of", render_ope(sup));
            }
            if ope_iri(sup) == Some(iri) {
                push(sections, "Super Property Of", render_sub_property(sub));
            }
        }
        Component::EquivalentObjectProperties(properties) => {
            if properties.0.iter().any(|ope| ope_iri(ope) == Some(iri)) {
                for ope in properties.0.iter().filter(|ope| ope_iri(ope) != Some(iri)) {
                    push(sections, "Equivalent To", render_ope(ope));
                }
            }
        }
        Component::DisjointObjectProperties(properties) => {
            if properties.0.iter().any(|ope| ope_iri(ope) == Some(iri)) {
                for ope in properties.0.iter().filter(|ope| ope_iri(ope) != Some(iri)) {
                    push(sections, "Disjoint With", render_ope(ope));
                }
            }
        }
        Component::InverseObjectProperties(InverseObjectProperties(first, second)) => {
            if first.0.as_ref() == iri {
                push(sections, "Inverse Of", render_iri(second.0.as_ref()));
            }
            if second.0.as_ref() == iri {
                push(sections, "Inverse Of", render_iri(first.0.as_ref()));
            }
        }
        Component::SubDataPropertyOf(SubDataPropertyOf { sup, sub }) => {
            if sub.0.as_ref() == iri {
                push(sections, "Sub Property Of", render_iri(sup.0.as_ref()));
            }
            if sup.0.as_ref() == iri {
                push(sections, "Super Property Of", render_iri(sub.0.as_ref()));
            }
        }
        Component::EquivalentDataProperties(properties) => {
            if properties.0.iter().any(|property| property.0.as_ref() == iri) {
                for property in properties.0.iter().filter(|property| property.0.as_ref() != iri) {
                    push(sections, "Equivalent To", render_iri(property.0.as_ref()));
                }
            }
        }
        Component::DisjointDataProperties(properties) => {
            if properties.0.iter().any(|property| property.0.as_ref() == iri) {
                for property in properties.0.iter().filter(|property| property.0.as_ref() != iri) {
                    push(sections, "Disjoint With", render_iri(property.0.as_ref()));
                }
            }
        }
        Component::DataPropertyDomain(DataPropertyDomain { dp, ce }) => {
            if dp.0.as_ref() == iri {
                push(sections, "Domain", render_ce(ce));
            } else if named_iri(ce) == Some(iri) {
                push(sections, "In Domain Of", render_iri(dp.0.as_ref()));
            }
        }
        Component::DataPropertyRange(DataPropertyRange { dp, dr }) => {
            if dp.0.as_ref() == iri {
                push(sections, "Range", render_dr(dr));
            }
        }
        Component::FunctionalDataProperty(dp) => {
            if dp.0.as_ref() == iri {
                push(sections, "Characteristics", "Functional".to_owned());
            }
        }
        Component::DatatypeDefinition(DatatypeDefinition { kind, range }) => {
            if kind.0.as_ref() == iri {
                push(sections, "Equivalent To", render_dr(range));
            }
        }
        Component::HasKey(HasKey { ce, vpe }) => {
            if named_iri(ce) == Some(iri) {
                push(sections, "Has Key", vpe.iter().map(render_property).collect::<Vec<_>>().join(", "));
            }
            if vpe.iter().any(|property| match property {
                PropertyExpression::ObjectPropertyExpression(ope) => ope_iri(ope) == Some(iri),
                PropertyExpression::DataProperty(dp) => dp.0.as_ref() == iri,
                PropertyExpression::AnnotationProperty(ap) => ap.0.as_ref() == iri,
            }) {
                push(sections, "Key For", render_ce(ce));
            }
        }
        Component::SameIndividual(individuals) => {
            if individuals.0.iter().any(|individual| individual_iri(individual) == Some(iri)) {
                for individual in individuals.0.iter().filter(|individual| individual_iri(individual) != Some(iri)) {
                    push(sections, "Same As", render_individual(individual));
                }
            }
        }
        Component::DifferentIndividuals(individuals) => {
            if individuals.0.iter().any(|individual| individual_iri(individual) == Some(iri)) {
                for individual in individuals.0.iter().filter(|individual| individual_iri(individual) != Some(iri)) {
                    push(sections, "Different From", render_individual(individual));
                }
            }
        }
        Component::Rule { .. } => push(sections, "Rules", render_component(component)),
        // Annotation assertions, declarations, imports, ontology metadata and plain property
        // assertions are already visible as normal RDF triples.
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
        if iri.starts_with("http://") || iri.starts_with("https://") || iri.starts_with("urn:") {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn example_ontology() -> SetOntology<ArcStr> {
        let mut reader = Cursor::new(EXAMPLE_KB);
        parse_rdf(&mut reader, Some(oxrdfio::RdfFormat::Turtle)).expect("the example knowledge base should parse")
    }

    #[test]
    fn parses_example_restriction() {
        let ontology = example_ontology();
        let rendered: Vec<_> = ontology.iter().map(|component| component.component.as_manchester().to_string()).collect();
        let equivalent = rendered.iter().find(|entry| entry.contains("EquivalentTo")).expect("the example should contain an equivalent class axiom");
        assert!(equivalent.contains("Age"), "{equivalent}");
        assert!(equivalent.contains("some"), "{equivalent}");
        assert!(equivalent.contains("decimal>"), "{equivalent}");
        assert!(equivalent.contains("[>="), "{equivalent}");
    }

    #[test]
    fn indexes_all_iris_of_a_component() {
        let ontology = example_ontology();
        let component = ontology
            .iter()
            .map(|component| &component.component)
            .find(|component| component.as_manchester().to_string().contains("EquivalentTo"))
            .expect("the example should contain an equivalent class axiom");
        let iris = component_iris(component);
        assert!(iris.contains("http://example.com/resource/ExClass"));
        assert!(iris.contains("http://example.com/resource/Age"));
    }
}
