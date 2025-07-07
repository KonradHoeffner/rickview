//! Load the RDF graph and summarize RDF resources.
#![allow(rustdoc::bare_urls)]
use crate::config::config;
use crate::resource::Resource;
#[cfg(feature = "hdt")]
use hdt::HdtGraph;
use log::*;
use multimap::MultiMap;
use sophia::api::graph::Graph;
use sophia::api::prefix::{Prefix, PrefixMap};
use sophia::api::prelude::{Triple, TripleSource};
use sophia::api::serializer::{Stringifier, TripleSerializer};
use sophia::api::term::bnode_id::BnodeId;
use sophia::api::term::matcher::{Any, TermMatcher};
use sophia::api::term::{FromTerm, SimpleTerm, Term};
use sophia::inmem::graph::FastGraph;
use sophia::iri::{Iri, IriRef};
use sophia::turtle::parser::{nt, turtle};
use sophia::turtle::serializer::nt::NtSerializer;
use sophia::turtle::serializer::turtle::{TurtleConfig, TurtleSerializer};
#[cfg(feature = "rdfxml")]
use sophia::xml::{self, serializer::RdfXmlSerializer};
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
    HdtGraph(HdtGraph),
}

impl GraphEnum {
    pub fn triples_matching<'s, S, P, O>(&'s self, sm: S, pm: P, om: O) -> Box<dyn Iterator<Item = Result<[SimpleTerm<'static>; 3], Infallible>> + 's>
    where
        S: TermMatcher + 's,
        P: TermMatcher + 's,
        O: TermMatcher + 's,
    {
        match self {
            // both graphs produce infallible results
            GraphEnum::FastGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
            #[cfg(feature = "hdt")]
            GraphEnum::HdtGraph(g) => Box::new(g.triples_matching(sm, pm, om).flatten().map(|triple| Ok(triple.map(SimpleTerm::from_term)))),
        }
    }
}

pub fn kb_reader(filename: &str) -> Result<BufReader<impl std::io::Read>, Box<dyn std::error::Error>> {
    Ok(BufReader::new(if filename.starts_with("http") {
        Box::new(ureq::get(filename).call()?.into_body().into_reader()) as Box<dyn std::io::Read>
    } else {
        Box::new(File::open(filename)?)
    }))
}

/// Load RDF graph from the RDF Turtle file specified in the config.
pub fn graph() -> &'static GraphEnum {
    GRAPH.get_or_init(|| {
        let t = Instant::now();
        let triples = match &config().kb_file {
            None => {
                warn!("No knowledge base configured. Loading example knowledge base. Set kb_file in data/config.toml or env var RICKVIEW_KB_FILE.");
                turtle::parse_str(EXAMPLE_KB).collect_triples()
            }
            Some(filename) => match kb_reader(filename) {
                Err(e) => {
                    error!("Cannot open '{filename}': {e}. Check kb_file data/config.toml or env var RICKVIEW_KB_FILE. EXITING RICKVIEW.");
                    std::process::exit(1);
                }
                Ok(br) => {
                    let br = BufReader::new(br);
                    match Path::new(&filename).extension().and_then(std::ffi::OsStr::to_str) {
                        Some("ttl") => turtle::parse_bufread(br).collect_triples(),
                        Some("nt") => nt::parse_bufread(br).collect_triples(),
                        // error types not compatible
                        #[cfg(feature = "rdfxml")]
                        Some("rdf" | "owl") => {
                            Ok(xml::parser::parse_bufread(br).collect_triples().unwrap_or_else(|e| panic!("Error parsing {filename} as RDF/XML: {e}")))
                        }
                        #[cfg(feature = "hdt")]
                        Some("zst") if filename.ends_with("hdt.zst") => {
                            let decoder = Decoder::with_buffer(br).expect("Error creating zstd decoder.");
                            let hdt = hdt::Hdt::new(BufReader::new(decoder)).expect("Error loading HDT.");
                            info!("Decompressed and loaded HDT from {filename} in {:?}", t.elapsed());
                            return GraphEnum::HdtGraph(hdt::HdtGraph::new(hdt));
                        }
                        #[cfg(feature = "hdt")]
                        Some("hdt") => {
                            let hdt_graph = hdt::HdtGraph::new(hdt::Hdt::new(br).unwrap_or_else(|e| panic!("Error loading HDT from {filename}: {e}")));
                            info!("Loaded HDT from {filename} in {:?}", t.elapsed());
                            return GraphEnum::HdtGraph(hdt_graph);
                        }
                        Some(ext) => {
                            error!("Unknown extension: \"{ext}\": cannot parse knowledge base. Aborting.");
                            std::process::exit(1);
                        }
                        None => {
                            warn!("{filename} has no extension: assuming RDF/XML.");
                            Ok(xml::parser::parse_bufread(br).collect_triples().unwrap_or_else(|e| panic!("Error parsing {filename} as RDF/XML: {e}")))
                        }
                    }
                }
            },
        };
        let g: FastGraph = triples.unwrap_or_else(|x| {
            error!("Unable to parse knowledge base {}: {}", &config().kb_file.as_deref().unwrap_or("example"), x);
            std::process::exit(1);
        });
        if log_enabled!(Level::Debug) {
            info!(
                "Loaded ~{} FastGraph triples from {} in {:?}",
                g.triples().size_hint().0,
                &config().kb_file.as_deref().unwrap_or("example kb"),
                t.elapsed()
            );
        }
        GraphEnum::FastGraph(g)
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
                            SimpleTerm::LiteralLanguage(lit, tag) => tagged.insert(tag.as_str().to_owned(), (uri, lit.to_string())),
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

fn blank_html(props: BTreeMap<String, Property>, depth: usize) -> String {
    if depth > 9 {
        return "...".to_owned();
    }
    // temporary manchester syntax emulation
    if let Some(on_property) = props.get("http://www.w3.org/2002/07/owl#onProperty")
        && let Some(some) = props.get("http://www.w3.org/2002/07/owl#someValuesFrom")
    {
        return format!("{} <b>some</b> {}", on_property.target_htmls.join(", "), some.target_htmls.join(", "));
    }
    let indent = "\n".to_owned() + &"\t".repeat(9 + depth);
    let indent2 = indent.clone() + "\t";
    #[allow(clippy::format_collect)]
    let rows = props
        .into_values()
        .map(|p| {
            format!(
                "{indent2}<tr><td class='td1'>{}</td><td class='td2'>{}</td></tr>",
                p.prop_html,
                p.target_htmls.into_iter().map(|h| format!("<span class='c2'>{h}</span>")).collect::<String>()
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
        let target_html = match target_term.as_simple() {
            SimpleTerm::LiteralLanguage(lit, tag) => format!("{lit} @{}", tag.as_str()),

            SimpleTerm::LiteralDatatype(lit, dt) => format!(r#"{lit}<div class="datatype">{}</div>"#, Piri::from(dt.as_ref()).short()),

            SimpleTerm::Iri(iri) => {
                let piri = Piri::from(iri.as_ref());
                let title = if let Some(title) = titles().get(&piri.to_string()) { format!("<br><span>&#8618; {title}</span>") } else { String::new() };
                let target = if piri.to_string().starts_with(config().namespace.as_str()) { "" } else { " target='_blank' " };
                format!("<a href='{}'{target}>{}{title}</a>", piri.root_relative(), piri.prefixed_string(false, true))
            }
            // https://www.w3.org/TR/rdf11-concepts/ Section 3.5 Replacing Blank Nodes with IRIs
            SimpleTerm::BlankNode(blank) => {
                let id = blank.as_str();
                let sub_html = if matches!(conn_type, PropertyType::Direct) {
                    blank_html(properties(&PropertyType::Direct, target_term, depth + 1), depth)
                } else {
                    String::new()
                };
                let r = IriRef::new_unchecked(SKOLEM_START.to_owned() + id);
                let iri = config().namespace.resolve(r);
                //format!("<a href='{}'>_:{id}</a><br>&#8618;<p>{sub_html}</p>", Piri::new(iri.as_ref()).root_relative())
                format!("&#8618;<a href='{}'> Blank Node {id}</a>{sub_html}", Piri::new(iri.as_ref()).root_relative())
            }
            _ => format!("{target_term:?}"),
        };
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
    Ok(RdfXmlSerializer::new_stringifier().serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as RDF Turtle using the config prefixes.
pub fn serialize_turtle(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    let config = TurtleConfig::new().with_pretty(true).with_own_prefix_map(prefixes().clone());
    Ok(TurtleSerializer::new_stringifier_with_config(config).serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
}

/// Export all triples (s,p,o) for a given subject s as N-Triples.
pub fn serialize_nt(iri: Iri<&str>) -> Result<String, Box<dyn Error>> {
    Ok(NtSerializer::new_stringifier().serialize_triples(graph().triples_matching(Some(deskolemize(&iri)), Any, Any))?.to_string())
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
    let title = titles().get(&piri.full).unwrap_or(&suffix).to_string().replace(SKOLEM_START, "Blank Node ");
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
        inverses,
        depiction: depiction_iri(subject),
    }
}
