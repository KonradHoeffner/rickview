//! Platform-neutral HTML rendering shared by the server and the WebAssembly viewer.
use crate::about::About;
use crate::config::{Config, config};
use crate::resource::Resource;
use serde::Serialize;
use sophia::iri::IriRef;
use tinytemplate::TinyTemplate;

static HEADER: &str = std::include_str!("../data/header.html");
static FOOTER: &str = std::include_str!("../data/footer.html");
static RESOURCE: &str = std::include_str!("../data/resource.html");
static INDEX: &str = std::include_str!("../data/index.html");
static ABOUT: &str = std::include_str!("../data/about.html");

const MIME_HTML: &str = "text/html";
const MIME_NT: &str = "application/n-triples";
const MIME_TTL: &str = "application/turtle";
#[cfg(feature = "rdfxml")]
const MIME_XML: &str = "application/rdf+xml";

#[derive(Serialize)]
pub struct Context {
    pub config: &'static Config,
    pub about: Option<About>,
    pub resource: Option<Resource>,
    pub classes: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Nt,
    Ttl,
    Html,
    #[cfg(feature = "rdfxml")]
    Xml,
}

pub struct RenderOutput {
    pub status: u16,
    pub content_type: &'static str,
    /// Complete HTML document for the server, or the serialized RDF.
    pub body: String,
    /// Body fragment for the WebAssembly viewer, or the serialized RDF.
    pub fragment: String,
}

pub fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("header", HEADER).expect("Could not parse header template");
    tt.add_template("footer", FOOTER).expect("Could not parse footer template");
    tt.add_template("resource", RESOURCE).expect("Could not parse resource page template");
    tt.add_template("index", INDEX).expect("Could not parse index page template");
    tt.add_template("about", ABOUT).expect("Could not parse about page template");
    tt.add_formatter("uri_to_suffix", |json, output| {
        let s = json.as_str().unwrap_or_else(|| panic!("JSON value is not a string: {json}"));
        let mut s = s.rsplit_once('/').unwrap_or_else(|| panic!("no '/' in URI '{s}'")).1;
        if s.contains('#') {
            s = s.rsplit_once('#').unwrap().1;
        }
        output.push_str(s);
        Ok(())
    });
    tt
}

fn render_parts(name: &str, context: &Context) -> Result<(String, String), tinytemplate::error::Error> {
    let tt = template();
    let fragment = tt.render(name, context)?;
    let mut body = tt.render("header", context.config)?;
    body += &fragment;
    body += &if name == "resource" { "</body></html>".to_owned() } else { tt.render("footer", context.config)? };
    Ok((body, fragment))
}

fn html_output(name: &str, context: &Context) -> RenderOutput {
    match render_parts(name, context) {
        Ok((body, fragment)) => RenderOutput { status: 200, content_type: MIME_HTML, body, fragment },
        Err(e) => error_output(name, e),
    }
}

fn text_output(status: u16, content_type: &'static str, body: String) -> RenderOutput {
    RenderOutput { status, content_type, fragment: body.clone(), body }
}

fn error_output(source: &str, error: impl std::fmt::Debug) -> RenderOutput {
    let message = format!("Could not render {source}: {error:?}");
    log::error!("{message}");
    text_output(500, "text/plain", message)
}

fn is_empty(res: &Resource) -> bool {
    res.descriptions.is_empty()
        && res.directs.is_empty()
        && res.duplicates.is_empty()
        && res.list.is_empty()
        && res.ontology.is_empty()
        && res.referencing.is_empty()
        && res.inverses.is_empty()
}

/// Whether `iri` is the knowledge base root, with or without trailing slash.
fn is_root(iri: &str) -> bool { iri.trim_end_matches('/') == config().namespace.as_str().trim_end_matches('/') }

/// Serve an RDF resource either as HTML or one of the serializations.
pub fn render_resource(suffix: &str, format: Format) -> RenderOutput {
    let prefixed = config().prefix.to_string() + ":" + suffix;
    let iri = config().namespace.resolve(IriRef::new_unchecked(suffix));
    let mut res = crate::rdf::resource(iri.as_ref());
    if is_empty(&res) {
        if suffix.is_empty() {
            // handle knowledge graphs with meta information in URI equal to the namespace with trailing slash removed
            let iri_noslash = sophia::iri::Iri::new_unchecked(iri.as_str().trim_end_matches('/'));
            res = crate::rdf::resource(iri_noslash);
            if is_empty(&res) && format == Format::Html {
                // index page is only shown as HTML and only if resource URI equal to namespace with or without slash does not exist
                return render_index();
            }
        } else {
            let warning = format!("No triples found for {suffix}. Did you configure the namespace correctly?");
            log::warn!("{warning}");
            if format == Format::Html {
                res.descriptions.push(("Warning".to_owned(), vec![warning.clone()]));
                // HTML is accepted and there are no errors, create a pseudo element in the empty resource to return 404 with HTML
                let context = Context { config: config(), about: None, resource: Some(res), classes: String::new() };
                let mut output = html_output("resource", &context);
                output.status = 404;
                return output;
            }
            return text_output(404, "text/plain", warning);
        }
    }

    match format {
        Format::Nt => match crate::rdf::serialize_nt(iri.as_ref()) {
            Ok(s) => text_output(200, MIME_NT, s),
            Err(e) => error_output(&format!("resource {prefixed}"), e),
        },
        Format::Ttl => match crate::rdf::serialize_turtle(iri.as_ref()) {
            Ok(s) => text_output(200, MIME_TTL, s),
            Err(e) => error_output(&format!("resource {prefixed}"), e),
        },
        #[cfg(feature = "rdfxml")]
        Format::Xml => match crate::rdf::serialize_rdfxml(iri.as_ref()) {
            Ok(s) => text_output(200, MIME_XML, s),
            Err(e) => error_output(&format!("resource {prefixed}"), e),
        },
        Format::Html => {
            let classes = if is_root(iri.as_str()) && !config().large { crate::classes::class_tree() } else { String::new() };
            let context = Context { config: config(), about: None, resource: Some(res), classes };
            html_output("resource", &context)
        }
    }
}

/// Render the index page, including the class tree.
pub fn render_index() -> RenderOutput {
    let classes = if config().large { String::new() } else { crate::classes::class_tree() };
    let context = Context { config: config(), about: None, classes, resource: None };
    html_output("index", &context)
}

/// Render the about page with graph statistics.
pub fn render_about() -> RenderOutput {
    let context = Context { config: config(), about: Some(About::new()), classes: String::new(), resource: None };
    html_output("about", &context)
}
