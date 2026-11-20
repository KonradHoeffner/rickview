//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::config::config;
use crate::resource::{OntologySection, Resource};
use anyhow::{Context, Result};
#[cfg(feature = "hdt")]
use hdt::Hdt;
use log::*;
use multimap::MultiMap;
#[cfg(feature = "rdfxml")]
use oxrdfxml::RdfXmlParser;
use sophia::api::graph::{Graph, MutableGraph};
use sophia::api::prefix::{Prefix, PrefixMap};
use sophia::api::prelude::Triple;
use sophia::api::serializer::{Stringifier, TripleSerializer};
use sophia::api::term::bnode_id::BnodeId;
use sophia::api::term::matcher::{Any, TermMatcher};
use sophia::api::term::{FromTerm, SimpleTerm, Term};
use sophia::inmem::graph::FastGraph;
use sophia::iri::{Iri, IriRef};
use sophia::turtle::parser::{nt, turtle};
use sophia::turtle::serializer::nt::NTriplesSerializer;
use sophia::turtle::serializer::turtle::{TurtleConfig, TurtleSerializer};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Instant;
#[cfg(feature = "hdt")]
use zstd::stream::read::Decoder;

static EXAMPLE_KB: &str = std::include_str!("../data/example.ttl");
static CAP: usize = 100; // maximum number of values shown per property
static SKOLEM_START: &str = ".well-known/genid/";

type PrefixItem = (Prefix<Box<str>>, Iri<Box<str>>);

// Prefixed IRI
pub struct Piri {
    full: String,
    prefixed: Option<(String, String)>,
}

impl fmt::Display for Piri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.full) }
}

/// Prefixed IRI
impl Piri {
    pub fn new(iri: Iri<&str>) -> Self {
        Self { prefixed: prefixes().get_prefixed_pair(iri).map(|(p, ms)| (p.to_string(), String::from(ms))), full: iri.as_str().to_owned() }
    }
    fn embrace(&self) -> String { format!("&lt;{self}&gt;") }
    fn prefixed_string(&self, bold: bool, embrace: bool) -> String {
        if let Some((p, s)) = &self.prefixed {
            if bold { format!("{p}:<b>{s}</b>") } else { format!("{p}:{s}") }
        } else if embrace {
            self.embrace()
        } else {
            self.to_string()
        }
    }
    pub fn short(&self) -> String { self.prefixed_string(false, false) }
    pub fn suffix(&self) -> String { self.prefixed.as_ref().map_or_else(|| self.full.clone(), |pair| pair.1.clone()) }
    pub fn root_relative(&self) -> String { self.full.replace(config().namespace.as_str(), &(config().base.clone() + "/")) }
    fn property_anchor(&self) -> String { format!("<a href='{}'>{}</a>", self.root_relative(), self.prefixed_string(true, false)) }

    pub(crate) fn link(&self) -> String {
        let title = if let Some(title) = titles().get(&self.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { String::new() };
        let target = if self.to_string().starts_with(config().namespace.as_str()) { "" } else { " target='_blank' " };
        format!("<a href='{}'{target}>{}{title}</a>", self.root_relative(), self.prefixed_string(false, true))
    }
}

impl From<&str> for Piri {
    fn from(s: &str) -> Piri { Piri::new(Iri::new_unchecked(s)) }
}

impl<T: std::borrow::Borrow<str>> From<&IriRef<T>> for Piri {
    fn from(iref: &IriRef<T>) -> Piri { Piri::new(Iri::new_unchecked(iref.as_str())) }
}

impl From<IriRef<&str>> for Piri {
    fn from(iref: IriRef<&str>) -> Piri { Piri::new(Iri::new_unchecked(&iref)) }
}

// Graph cannot be made into a trait object as of Rust 1.67 and Sophia 0.7, see https://github.com/pchampin/sophia_rs/issues/122.
// Enum is cumbersome but we don't have a choice.
// There may be a more elegant way in future Rust and Sophia versions.
#[allow(clippy::large_enum_variant)]
pub enum GraphEnum {
    // Sophia: "A heavily indexed graph. Fast to query but slow to load, with a relatively high memory footprint.".
    // Alternatively, use LightGraph, see <https://docs.rs/sophia/latest/sophia/graph/inmem/type.LightGraph.html>.
    FastGraph(FastGraph),
    #[cfg(feature = "hdt")]
    HdtGraph(Hdt),
}

impl GraphEnum {
    pub fn triples_matching<'s, 't, S, P, O>(&'s self, sm: S, pm: P, om: O) -> Box<dyn Iterator<Item = Result<[SimpleTerm<'static>; 3], Infallible>> + 't>
    where
        's: 't,
        S: TermMatcher + 't,
        P: TermMatcher + 't,
        O: TermMatcher + 't,
    {
        match self {
            // both graphs produce infallible results
            GraphEnum::FastGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
            #[cfg(feature = "hdt")]
            GraphEnum::HdtGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
        }
    }
}

pub fn kb_reader(filename: &str) -> anyhow::Result<BufReader<impl std::io::Read>> {
    Ok(BufReader::new(if filename.starts_with("http") {
        Box::new(ureq::get(filename).call()?.into_body().into_reader()) as Box<dyn std::io::Read>
    } else {
        Box::new(File::open(filename)?)
    }))
}

/// Convert an RDF/XML triple into terms that `FastGraph` can store.
#[cfg(feature = "rdfxml")]
fn load_rdfxml(g: &mut FastGraph, reader: impl std::io::Read) -> anyhow::Result<usize> {
    use oxrdf::{NamedOrBlankNode, Term as OxTerm};
    use sophia::api::MownStr;
    use sophia::api::term::LanguageTag;

    let mut count = 0;
    for triple in RdfXmlParser::new().for_reader(reader) {
        let triple = triple.context("Error parsing RDF/XML")?;
        let subject = match &triple.subject {
            NamedOrBlankNode::NamedNode(node) => SimpleTerm::Iri(IriRef::new_unchecked(MownStr::from(node.as_str()))),
            NamedOrBlankNode::BlankNode(node) => SimpleTerm::BlankNode(BnodeId::new_unchecked(MownStr::from(node.as_str()))),
        };
        let predicate = SimpleTerm::Iri(IriRef::new_unchecked(MownStr::from(triple.predicate.as_str())));
        let object = match &triple.object {
            OxTerm::NamedNode(node) => SimpleTerm::Iri(IriRef::new_unchecked(MownStr::from(node.as_str()))),
            OxTerm::BlankNode(node) => SimpleTerm::BlankNode(BnodeId::new_unchecked(MownStr::from(node.as_str()))),
            OxTerm::Literal(literal) => {
                let value = MownStr::from(literal.value());
                if let Some(language) = literal.language() {
                    SimpleTerm::LiteralLanguage(value, LanguageTag::new_unchecked(MownStr::from(language)), None)
                } else {
                    SimpleTerm::LiteralDatatype(value, IriRef::new_unchecked(MownStr::from(literal.datatype().as_str())))
                }
            }
        };
        g.insert(subject, predicate, object)?;
        count += 1;
    }
    Ok(count)
}

/// Convert a sophia term into an oxrdf term for RDF/XML serialization.
#[cfg(feature = "rdfxml")]
fn to_ox_term(term: &SimpleTerm<'_>) -> anyhow::Result<oxrdf::Term> {
    use oxrdf::{BlankNode, Literal, NamedNode, Term as OxTerm};

    Ok(match term {
        SimpleTerm::Iri(iri) => OxTerm::NamedNode(NamedNode::new(iri.as_str())?),
        SimpleTerm::BlankNode(node) => OxTerm::BlankNode(BlankNode::new(node.as_str())?),
        SimpleTerm::LiteralDatatype(value, datatype) => OxTerm::Literal(Literal::new_typed_literal(value.as_ref(), NamedNode::new(datatype.as_str())?)),
        SimpleTerm::LiteralLanguage(value, language, _) => OxTerm::Literal(Literal::new_language_tagged_literal(value.as_ref(), language.as_str())?),
        _ => anyhow::bail!("Unsupported RDF term in RDF/XML serialization"),
    })
}

fn load_dir(g: &mut FastGraph, filename: &str) -> anyhow::Result<usize> {
    info!("Loading all .ttl and .nt files from directory {filename}");
    std::fs::read_dir(filename)
        .context("Failed to read directory")?
        .flatten()
        .map(|e| -> anyhow::Result<usize> {
            let p = e.path();

            match p.extension().and_then(std::ffi::OsStr::to_str) {
                Some(ext @ ("ttl" | "nt" | "rdf" | "owl")) => {
                    let br = BufReader::new(File::open(&p).context("Error opening file")?);
                    match ext {
                        "ttl" => g.insert_all(turtle::parse_bufread(br)).context("Error parsing Turtle"),
                        "nt" => g.insert_all(nt::parse_bufread(br)).context("Error loading N-Triples"),
                        #[cfg(feature = "rdfxml")]
                        "rdf" | "owl" => load_rdfxml(g, br).context("Error loading RDF/XML"),
                        _ => Ok(0),
                    }
                }
                .context("Failed to parse file"),
                _ => Ok(0),
            }
            .with_context(|| format!("Failed to read file {} within directory", p.display()))
        })
        .sum()
}

fn load_graph() -> anyhow::Result<GraphEnum> {
    //let load = || -> anyhow::Result<(FastGraph, usize)> {
    let mut g = FastGraph::new();
    let t = Instant::now();
    let num_triples = match &config().kb_file {
        None => {
            warn!("No knowledge base configured. Loading example knowledge base. Set kb_file in data/config.toml or env var RICKVIEW_KB_FILE.");
            g.insert_all(turtle::parse_str(EXAMPLE_KB)).context("Failed to parse example knowledge base")?
        }
        Some(filename) if std::fs::metadata(filename).is_ok_and(|m| m.is_dir()) => load_dir(&mut g, filename)?,
        Some(filename) => {
            let br = kb_reader(filename).context("Cannot read from file or URL.")?;
            match Path::new(&filename).extension().and_then(std::ffi::OsStr::to_str) {
                Some("ttl") => g.insert_all(turtle::parse_bufread(br)).context("Failed to parse Turtle"),
                // error types not compatible
                #[cfg(feature = "rdfxml")]
                Some("rdf" | "owl") | None => load_rdfxml(&mut g, br).context("Failed to parse RDF/XML"),
                #[allow(unreachable_patterns)] // None in case feature "rdfxml" is not active
                Some("nt") | None => g.insert_all(nt::parse_bufread(br)).context("Failed to parse N-Triples"),
                #[cfg(feature = "hdt")]
                Some("zst") if filename.ends_with("hdt.zst") => {
                    let decoder = Decoder::with_buffer(br).context("Failed to create zstd decoder")?;
                    let hdt = hdt::Hdt::read(BufReader::new(decoder)).context("Failed to load HDT")?;
                    info!("Decompressed and loaded HDT from {filename} in {:?}", t.elapsed());
                    return Ok(GraphEnum::HdtGraph(hdt));
                }
                #[cfg(feature = "hdt")]
                Some("hdt") => {
                    let hdt = Hdt::read(br).context("Failed to load HDT")?;
                    info!("Loaded HDT from {filename} in {:?}", t.elapsed());
                    return Ok(GraphEnum::HdtGraph(hdt));
                }
                Some(ext) => Err(anyhow::anyhow!("Unknown extension: \"{ext}\": cannot parse knowledge graph.")),
            }
            .with_context(|| format!("Failed to load file '{filename}'"))?
        }
    };
    if num_triples == 0 {
        return Err(anyhow::anyhow!("Graph is empty"));
    }
    if log_enabled!(Level::Info) {
        info!("Loaded {} FastGraph triples from {} in {:?}", num_triples, config().kb_file.as_deref().unwrap_or("example kb"), t.elapsed());
    }
    Ok(GraphEnum::FastGraph(g))
}

/// Load RDF graph from the RDF Turtle file specified in the config.
pub fn graph() -> &'static GraphEnum {
    //let (g, num_triples) = load()
    GRAPH.get_or_init(|| {
        load_graph().unwrap_or_else(|e| {
            error!("Fatal error loading graph from {}: {e:?}", config().kb_file.as_deref().unwrap_or("example kb"));
            std::process::exit(1);
        })
    })
}

/// (prefix,iri) pairs from the config
fn prefixes() -> &'static Vec<PrefixItem> {
    PREFIXES.get_or_init(|| {
        let mut p: Vec<PrefixItem> = Vec::new();
        for (prefix, iri) in &config().namespaces {
            p.push((Prefix::new_unchecked(prefix.clone()), Iri::new_unchecked(iri.clone())));
        }
        p.push((Prefix::new_unchecked(config().prefix.clone()), config().namespace.clone()));
        p
    })
}

/// Maps RDF resource URIs to at most one title each, for example `http://example.com/resource/ExampleResource` -> "example resource".
/// Prioritizes `title_properties` earlier in the list.
/// This is only run once to minimize the number of queries and generates the title for every resource in the graph.
/// For very large graph this can take too much time or memory and can be disabled with setting the "large" config option to true.
pub fn titles() -> &'static HashMap<String, String> {
    TITLES.get_or_init(|| {
        let mut titles = HashMap::<String, String>::new();
        if config().large {
            return titles;
        }
        let g = graph();
        // tag, uri, title
        let mut tagged = MultiMap::<String, (String, String)>::new();
        for prop in config().title_properties.iter().rev() {
            match IriRef::new(prop.clone().into()) {
                Err(_) => {
                    error!("Skipping invalid title property {prop}");
                }
                Ok(iref) => {
                    let term = SimpleTerm::Iri(iref);
                    for tt in g.triples_matching(Any, Some(term), Any) {
                        let t = tt.expect("error fetching title triple");
                        // ignore blank node labels as title because they usually don't have any
                        if t.s().is_blank_node() {
                            continue;
                        }
                        let uri = t.s().as_simple().iri().expect("invalid title subject IRI").as_str().to_owned();
                        match t.o() {
                            SimpleTerm::LiteralLanguage(lit, tag, _) => tagged.insert(tag.as_str().to_owned(), (uri, lit.to_string())),
                            SimpleTerm::LiteralDatatype(lit, _) => tagged.insert(String::new(), (uri, lit.to_string())),
                            _ => warn!("Invalid title value {:?}, skipping", t.o().as_simple()),
                        }
                    }
                }
            }
        }
        // prioritize language tags listed earlier in config().langs
        let mut tags: Vec<&String> = tagged.keys().collect();
        tags.sort_by_cached_key(|tag| config().langs.iter().position(|x| &x == tag).unwrap_or(1000));
        tags.reverse();
        for tag in tags {
            if let Some(v) = tagged.get_vec(tag) {
                for (uri, title) in v {
                    titles.insert(uri.clone(), title.clone());
                }
            }
        }
        titles
    })
}

/// Maps RDF resource suffixes to at most one type URI each, for example "`ExampleResource`" -> `http://example.com/resource/ExampleClass`.
/// Prioritizes `type_properties` earlier in the list.
pub fn types() -> &'static HashMap<String, String> {
    TYPES.get_or_init(|| {
        let mut types = HashMap::<String, String>::new();
        if config().large {
            return types;
        }
        for prop in config().type_properties.iter().rev() {
            let iref = IriRef::new(prop.clone().into());
            if iref.is_err() {
                error!("invalid type property {prop}");
                continue;
            }
            let term = SimpleTerm::Iri(iref.unwrap());
            for tt in graph().triples_matching(Any, Some(term), Any) {
                let t = tt.expect("error fetching type triple");
                if !t.s().is_iri() {
                    continue;
                }
                let suffix = t.s().as_simple().iri().expect("invalid type subject IRI").to_string().replace(config().namespace.as_str(), "");
                match t.o().as_simple() {
                    SimpleTerm::Iri(iri) => {
                        types.insert(suffix, iri.to_string());
                    }
                    _ => {
                        warn!("Skipping invalid type {:?} for suffix {suffix} with property <{prop}>.", t.o().as_simple());
                    }
                }
            }
        }
        types
    })
}

/// Contains the knowledge base.
static GRAPH: OnceLock<GraphEnum> = OnceLock::new();
static PREFIXES: OnceLock<Vec<PrefixItem>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one title each.
static TITLES: OnceLock<HashMap<String, String>> = OnceLock::new();
/// Map of RDF resource suffixes to at most one type URI each. Result of [types].
static TYPES: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Whether the given resource is in subject or object position.
enum PropertyType {
    Direct,
    Inverse,
}

#[derive(Debug)]
struct Property {
    prop_html: String,
    target_htmls: Vec<String>,
}

impl From<Property> for (String, Vec<String>) {
    fn from(p: Property) -> Self { (p.prop_html, p.target_htmls) }
}

/// Map skolemized IRIs back to blank nodes. Keep deskolemized IRIs as they are.
fn deskolemize<'a>(iri: &'a Iri<&str>) -> SimpleTerm<'a> {
    if let Some(id) = iri.as_str().split(SKOLEM_START).nth(1) { SimpleTerm::from_term(BnodeId::new_unchecked(id.to_owned())) } else { iri.as_simple() }
}

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";
const OWL_ON_PROPERTY: &str = "http://www.w3.org/2002/07/owl#onProperty";
const OWL_ON_DATATYPE: &str = "http://www.w3.org/2002/07/owl#onDatatype";
const OWL_WITH_RESTRICTIONS: &str = "http://www.w3.org/2002/07/owl#withRestrictions";
const OWL_SOME_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#someValuesFrom";
const OWL_ALL_VALUES_FROM: &str = "http://www.w3.org/2002/07/owl#allValuesFrom";
const OWL_HAS_VALUE: &str = "http://www.w3.org/2002/07/owl#hasValue";
const OWL_HAS_SELF: &str = "http://www.w3.org/2002/07/owl#hasSelf";
const OWL_MIN_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minCardinality";
const OWL_MAX_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxCardinality";
const OWL_EXACT_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#cardinality";
const OWL_MIN_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#minQualifiedCardinality";
const OWL_MAX_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#maxQualifiedCardinality";
const OWL_EXACT_QUALIFIED_CARDINALITY: &str = "http://www.w3.org/2002/07/owl#qualifiedCardinality";
const OWL_ON_CLASS: &str = "http://www.w3.org/2002/07/owl#onClass";
const OWL_ON_DATA_RANGE: &str = "http://www.w3.org/2002/07/owl#onDataRange";
const OWL_INTERSECTION_OF: &str = "http://www.w3.org/2002/07/owl#intersectionOf";
const OWL_UNION_OF: &str = "http://www.w3.org/2002/07/owl#unionOf";
const OWL_COMPLEMENT_OF: &str = "http://www.w3.org/2002/07/owl#complementOf";
const OWL_ONE_OF: &str = "http://www.w3.org/2002/07/owl#oneOf";

fn html_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }

/// All direct (predicate, object) pairs of a subject, keeping the original terms so they can be
/// rendered semantically (for example as OWL restrictions).
fn direct_terms(source: &SimpleTerm<'_>) -> BTreeMap<String, Vec<SimpleTerm<'static>>> {
    let mut map = BTreeMap::<String, Vec<SimpleTerm<'static>>>::new();
    for res in graph().triples_matching(Some(source), Any, Any) {
        let triple = res.expect("error with connection triple");
        if let SimpleTerm::Iri(iri) = triple.p().as_simple() {
            map.entry(iri.as_str().to_owned()).or_default().push(triple.o().clone());
        }
    }
    map
}

fn term_html(term: &SimpleTerm<'_>, depth: usize) -> String {
    match term.as_simple() {
        SimpleTerm::LiteralLanguage(lit, tag, _) => format!("{} @{}", html_escape(lit.as_ref()), tag.as_str()),
        SimpleTerm::LiteralDatatype(lit, dt) => {
            format!(r#"{}<div class="datatype">{}</div>"#, html_escape(lit.as_ref()), Piri::from(dt.as_ref()).short())
        }
        SimpleTerm::Iri(iri) => Piri::from(iri.as_ref()).link(),
        SimpleTerm::BlankNode(blank) => {
            let id = blank.as_str();
            let r = IriRef::new_unchecked(SKOLEM_START.to_owned() + id);
            let iri = config().namespace.resolve(r);
            let sub_html = blank_html(term, depth + 1);
            format!("&#8618;<a href='{}'> Blank Node {id}</a> {sub_html}", Piri::new(iri.as_ref()).root_relative())
        }
        _ => format!("{term:?}"),
    }
}

/// Render a term inside an expression without the "Blank Node" label.
fn term_html_inner(term: &SimpleTerm<'_>, depth: usize) -> String {
    match term.as_simple() {
        SimpleTerm::BlankNode(_) => blank_html(term, depth + 1),
        _ => term_html(term, depth),
    }
}

fn first_term<'a>(props: &'a BTreeMap<String, Vec<SimpleTerm<'static>>>, iri: &str) -> Option<&'a SimpleTerm<'static>> {
    props.get(iri).and_then(|values| values.first())
}

fn literal_value(term: &SimpleTerm<'_>) -> Option<String> {
    match term.as_simple() {
        SimpleTerm::LiteralDatatype(lit, _) | SimpleTerm::LiteralLanguage(lit, ..) => Some(lit.to_string()),
        _ => None,
    }
}

/// Read an RDF collection starting at `node`.
fn list_items(node: &SimpleTerm<'_>) -> Vec<SimpleTerm<'static>> {
    let mut items = Vec::new();
    let mut current = node.clone();
    for _ in 0..CAP {
        if let SimpleTerm::Iri(iri) = current.as_simple()
            && iri.as_str() == RDF_NIL
        {
            break;
        }
        let props = direct_terms(&current);
        let Some(first) = first_term(&props, RDF_FIRST) else { break };
        items.push(first.clone());
        let Some(rest) = first_term(&props, RDF_REST) else { break };
        current = rest.clone();
        if let SimpleTerm::BlankNode(_) = current.as_simple() {
            continue;
        }
        if let SimpleTerm::Iri(iri) = current.as_simple()
            && iri.as_str() == RDF_NIL
        {
            break;
        }
        break;
    }
    items
}

/// Render a term and report whether it needs parentheses when used as an operand.
fn operand_html(term: &SimpleTerm<'_>, depth: usize) -> (String, bool) {
    let html = term_html_inner(term, depth);
    (html, matches!(term.as_simple(), SimpleTerm::BlankNode(_)))
}

fn join_operands(terms: &[SimpleTerm<'static>], separator: &str, depth: usize) -> String {
    terms
        .iter()
        .map(|term| {
            let (html, parens) = operand_html(term, depth + 1);
            if parens { format!("({html})") } else { html }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

fn cardinality(props: &BTreeMap<String, Vec<SimpleTerm<'static>>>, key: &str, keyword: &str, qualifier: Option<&str>, depth: usize) -> Option<String> {
    let n = literal_value(first_term(props, key)?)?;
    let property = first_term(props, OWL_ON_PROPERTY).map(|p| term_html_inner(p, depth + 1)).unwrap_or_default();
    let qualifier = qualifier.and_then(|q| first_term(props, q)).map(|q| term_html_inner(q, depth + 1));
    Some(match qualifier {
        Some(q) => format!("{keyword} {n} {property} {q}"),
        None => format!("{keyword} {n} {property}"),
    })
}

fn facet_symbol(iri: &str) -> String {
    let local = iri.rsplit(['#', '/']).next().unwrap_or(iri);
    match local {
        "minInclusive" => ">=".to_owned(),
        "maxInclusive" => "<=".to_owned(),
        "minExclusive" => ">".to_owned(),
        "maxExclusive" => "<".to_owned(),
        "pattern" => "pattern".to_owned(),
        "length" => "length".to_owned(),
        "minLength" => "minLength".to_owned(),
        "maxLength" => "maxLength".to_owned(),
        "langRange" => "langRange".to_owned(),
        "totalDigits" => "totalDigits".to_owned(),
        "fractionDigits" => "fractionDigits".to_owned(),
        _ => local.to_owned(),
    }
}

fn blank_html(node: &SimpleTerm<'_>, depth: usize) -> String {
    if depth > 9 {
        return "...".to_owned();
    }
    let props = direct_terms(node);

    // OWL restriction, for example `has_value some decimal[>= 18, < 34]`.
    if let Some(on_property) = first_term(&props, OWL_ON_PROPERTY) {
        let property = term_html_inner(on_property, depth + 1);
        if let Some(filler) = first_term(&props, OWL_SOME_VALUES_FROM) {
            return format!("{property} <b>some</b> {}", term_html_inner(filler, depth + 1));
        }
        if let Some(filler) = first_term(&props, OWL_ALL_VALUES_FROM) {
            return format!("{property} <b>only</b> {}", term_html_inner(filler, depth + 1));
        }
        if let Some(value) = first_term(&props, OWL_HAS_VALUE) {
            return format!("{property} <b>value</b> {}", term_html_inner(value, depth + 1));
        }
        if first_term(&props, OWL_HAS_SELF).is_some() {
            return format!("{property} <b>Self</b>");
        }
        if let Some(s) = cardinality(&props, OWL_MIN_CARDINALITY, "min", Some(OWL_ON_CLASS), depth).or_else(|| {
            cardinality(&props, OWL_MIN_QUALIFIED_CARDINALITY, "min", Some(OWL_ON_CLASS), depth)
                .or_else(|| cardinality(&props, OWL_MIN_QUALIFIED_CARDINALITY, "min", Some(OWL_ON_DATA_RANGE), depth))
        }) {
            return s;
        }
        if let Some(s) = cardinality(&props, OWL_MAX_CARDINALITY, "max", Some(OWL_ON_CLASS), depth).or_else(|| {
            cardinality(&props, OWL_MAX_QUALIFIED_CARDINALITY, "max", Some(OWL_ON_CLASS), depth)
                .or_else(|| cardinality(&props, OWL_MAX_QUALIFIED_CARDINALITY, "max", Some(OWL_ON_DATA_RANGE), depth))
        }) {
            return s;
        }
        if let Some(s) = cardinality(&props, OWL_EXACT_CARDINALITY, "exactly", Some(OWL_ON_CLASS), depth).or_else(|| {
            cardinality(&props, OWL_EXACT_QUALIFIED_CARDINALITY, "exactly", Some(OWL_ON_CLASS), depth)
                .or_else(|| cardinality(&props, OWL_EXACT_QUALIFIED_CARDINALITY, "exactly", Some(OWL_ON_DATA_RANGE), depth))
        }) {
            return s;
        }
    }

    // Datatype with facets, for example `xsd:decimal[>= 18, < 34]`.
    if let Some(datatype) = first_term(&props, OWL_ON_DATATYPE) {
        let base = term_html_inner(datatype, depth + 1);
        if let Some(restrictions) = first_term(&props, OWL_WITH_RESTRICTIONS) {
            let facets = list_items(restrictions)
                .iter()
                .filter_map(|facet| {
                    let facet_props = direct_terms(facet);
                    let (predicate, value) = facet_props.iter().next()?;
                    let value = literal_value(value.first()?)?;
                    Some(format!("{} {}", facet_symbol(predicate), html_escape(&value)))
                })
                .collect::<Vec<_>>();
            if !facets.is_empty() {
                return format!("{base}[{}]", facets.join(", "));
            }
        }
        return base;
    }

    for (key, separator) in [(OWL_INTERSECTION_OF, " and "), (OWL_UNION_OF, " or ")] {
        if let Some(list) = first_term(&props, key) {
            return join_operands(&list_items(list), separator, depth);
        }
    }
    if let Some(inner) = first_term(&props, OWL_COMPLEMENT_OF) {
        let (html, parens) = operand_html(inner, depth + 1);
        return if parens { format!("not ({html})") } else { format!("not {html}") };
    }
    if let Some(individuals) = first_term(&props, OWL_ONE_OF) {
        return format!("{{{}}}", list_items(individuals).iter().map(|i| term_html_inner(i, depth + 1)).collect::<Vec<_>>().join(", "));
    }

    // Plain RDF list, for example `(a b c)`.
    if props.contains_key(RDF_FIRST) {
        return format!("({})", list_items(node).iter().map(|i| term_html_inner(i, depth + 1)).collect::<Vec<_>>().join(" "));
    }

    // Fall back to a table for unknown blank node structures.
    let indent = "\n".to_owned() + &"\t".repeat(9 + depth);
    let indent2 = indent.clone() + "\t";
    #[allow(clippy::format_collect)]
    let rows = props
        .into_iter()
        .map(|(prop, values)| {
            let len = values.len();
            let mut target_htmls: Vec<String> = values.iter().take(CAP).map(|v| term_html(v, depth + 1)).collect();
            if len > CAP {
                target_htmls.push("...".to_owned());
            }
            format!(
                "{indent2}<tr><td class='td1'>{}</td><td class='td2'>{}</td></tr>",
                Piri::new(Iri::new_unchecked(&prop)).property_anchor(),
                target_htmls.into_iter().map(|h| format!("<span class='c2'>{h}</span>")).collect::<String>()
            )
        })
        .collect::<String>();
    format!("{indent}<table>{rows}{indent}</table>")
}

/// For a given resource r, get either all direct properties (p,o) where (r,p,o) is in the graph or indirect ones (s,p) where (s,p,r) is in the graph.
fn properties(conn_type: &PropertyType, source: &SimpleTerm<'_>, depth: usize) -> BTreeMap<String, Property> {
    let g = graph();
    let triples = match conn_type {
        PropertyType::Direct => g.triples_matching(Some(source), Any, Any),
        PropertyType::Inverse => g.triples_matching(Any, Any, Some(source)),
    };
    let mut map: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for res in triples {
        let triple = res.expect("error with connection triple");
        let target_term = match conn_type {
            PropertyType::Direct => triple.o(),
            PropertyType::Inverse => triple.s(),
        };
        let target_html = term_html(target_term, depth);
        if let SimpleTerm::Iri(iri) = triple.p().as_simple() {
            if let Some(values) = map.get_mut(iri.as_str()) {
                values.insert(target_html);
            } else {
                let mut values = BTreeSet::new();
                values.insert(target_html);
                map.insert(iri.as_str().to_owned(), values);
            }
        }
    }
    map.into_iter()
        .map(|(prop, values)| {
            let len = values.len();
            let mut target_htmls: Vec<String> = values.into_iter().take(CAP).collect();
            if len > CAP {
                target_htmls.push("...".to_string());
            }
            (prop.clone(), Property { prop_html: Piri::new(Iri::new_unchecked(&prop)).property_anchor(), target_htmls })
        })
        .collect()
}

#[cfg(feature = "rdfxml")]
/// Export all triples (s,p,o) for a given subject s as RDF/XML.
pub fn serialize_rdfxml(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    let mut serializer = oxrdfxml::RdfXmlSerializer::new().for_writer(Vec::new());
    for triple in graph().triples_matching(Some(deskolemize(&iri)), Any, Any) {
        let [subject, predicate, object] = triple?.to_spo();
        let subject = match to_ox_term(&subject)? {
            oxrdf::Term::NamedNode(node) => oxrdf::NamedOrBlankNode::NamedNode(node),
            oxrdf::Term::BlankNode(node) => oxrdf::NamedOrBlankNode::BlankNode(node),
            oxrdf::Term::Literal(_) => return Err("RDF/XML subject must be a named or blank node".into()),
        };
        let oxrdf::Term::NamedNode(predicate) = to_ox_term(&predicate)? else {
            return Err("RDF/XML predicate must be a named node".into());
        };
        let object = to_ox_term(&object)?;
        let triple = oxrdf::Triple::new(subject, predicate, object);
        serializer.serialize_triple(&triple)?;
    }
    Ok(String::from_utf8(serializer.finish()?)?)
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map(prefixes().clone());
    Ok(TurtleSerializer::new_stringifier_with_config(config).serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    Ok(NTriplesSerializer::new_stringifier().serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

fn depiction_iri(iri: Iri<&str>) -> Option<String> {
    let foaf_depiction = IriRef::new_unchecked("http://xmlns.com/foaf/0.1/depiction");
    graph()
        .triples_matching(Some(iri), Some(foaf_depiction), Any)
        .filter_map(Result::ok)
        .map(Triple::to_o)
        .filter(Term::is_iri)
        .map(|o| o.iri().unwrap().as_str().to_owned())
        .next()
}

#[cfg(feature = "owl")]
fn ontology_sections(iri: &str) -> Vec<OntologySection> { crate::owl::sections(iri) }

#[cfg(not(feature = "owl"))]
fn ontology_sections(_iri: &str) -> Vec<OntologySection> { Vec::new() }

/// Returns the resource with the given IRI from the configured namespace.
pub fn resource(subject: Iri<&str>) -> Resource {
    let start = Instant::now();
    let piri = Piri::new(subject.as_ref());
    let suffix = piri.suffix();
    let convert = |m: BTreeMap<String, Property>| -> Vec<_> { m.into_values().map(Property::into).collect() };

    let source = deskolemize(&subject);
    let mut all_directs = properties(&PropertyType::Direct, &source, 0);
    let descriptions = convert(config().description_properties.iter().filter_map(|p| all_directs.remove_entry(p)).collect());
    let directs = convert(all_directs);
    let title = titles().get(&piri.full).unwrap_or(&suffix).clone().replace(SKOLEM_START, "Blank Node ");
    let ontology = ontology_sections(&piri.full);
    let main_type = types().get(&suffix).cloned();
    let inverses = if config().show_inverse { convert(properties(&PropertyType::Inverse, &source, 0)) } else { Vec::new() };
    Resource {
        uri: piri.full,
        base: config().base.clone(),
        duration: format!("{:?}", start.elapsed()),
        title,
        github_issue_url: config().github.as_ref().map(|g| format!("{g}/issues/new?title={suffix}")),
        main_type,
        descriptions,
        directs,
        ontology,
        inverses,
        depiction: depiction_iri(subject),
    }
}

#[cfg(test)]
mod tests {
    use super::{facet_symbol, html_escape};

    #[test]
    fn escapes_html() {
        assert_eq!(html_escape("a & b < c > d"), "a &amp; b &lt; c &gt; d");
    }

    #[test]
    fn maps_datatype_facets() {
        assert_eq!(facet_symbol("http://www.w3.org/2001/XMLSchema#minInclusive"), ">=");
        assert_eq!(facet_symbol("http://www.w3.org/2001/XMLSchema#maxExclusive"), "<");
        assert_eq!(facet_symbol("http://www.w3.org/2001/XMLSchema#pattern"), "pattern");
    }
}
