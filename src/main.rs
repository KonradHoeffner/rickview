#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::similar_names)]
#![deny(rust_2018_idioms)]
//! Lightweight and performant RDF browser.
//! An RDF browser is a web application that *resolves* RDF resources: given the HTTP(s) URL identifying a resource it returns an HTML summary.
//! Besides HTML, the RDF serialization formats RDF/XML, Turtle and N-Triples are also available using content negotiation.
//! Default configuration is stored in `data/default.toml`, which can be overriden in `data/config.toml` or environment variables.
//! Configuration keys are in `lower_snake_case`, while environment variables are prefixed with RICKVIEW\_ and are `in SCREAMING_SNAKE_CASE`.
mod about;
mod classes;
/// The main module uses Actix Web to serve resources as HTML and other formats.
mod config;
mod rdf;
mod resource;

use crate::config::{Config, config};
use crate::resource::Resource;
use about::About;
use actix_web::body::MessageBody;
use actix_web::http::header::{self, ETag, EntityTag};
use actix_web::middleware::Compress;
use actix_web::web::scope;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, get, head, web};
use const_fnv1a_hash::{fnv1a_hash_32, fnv1a_hash_str_32};
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use sophia::iri::IriRef;
use std::error::Error;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tinytemplate::TinyTemplate;

static HEADER: &str = std::include_str!("../data/header.html");
static FOOTER: &str = std::include_str!("../data/footer.html");
static RESOURCE: &str = std::include_str!("../data/resource.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
// extremely low risk of collision, worst case is out of date favicon or CSS
static FAVICON_HASH: u32 = fnv1a_hash_32(FAVICON, None);
static RICKVIEW_CSS: &str = std::include_str!("../data/rickview.css");
static RICKVIEW_CSS_HASH: u32 = fnv1a_hash_str_32(RICKVIEW_CSS);
static ROBOTO_CSS: &str = std::include_str!("../data/roboto.css");
static ROBOTO_CSS_HASH: u32 = fnv1a_hash_str_32(ROBOTO_CSS);
static ROBOTO300: &[u8] = std::include_bytes!("../fonts/roboto300.woff2");
static INDEX: &str = std::include_str!("../data/index.html");
static ABOUT: &str = std::include_str!("../data/about.html");
static CUSTOM: &str = std::include_str!("../data/custom.html");
static RUN_ID: AtomicU32 = AtomicU32::new(0);

// 8 chars hexadecimal, not worth it to add base64 dependency to save 2 chars
static FAVICON_SHASH: LazyLock<String> = LazyLock::new(|| format!("{FAVICON_HASH:x}"));
static FAVICON_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *FAVICON_SHASH));
static RICKVIEW_CSS_SHASH: LazyLock<String> = LazyLock::new(|| format!("{RICKVIEW_CSS_HASH:x}"));
static RICKVIEW_CSS_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *RICKVIEW_CSS_SHASH));
static ROBOTO_CSS_SHASH: LazyLock<String> = LazyLock::new(|| format!("{ROBOTO_CSS_HASH:x}"));
static ROBOTO_CSS_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *ROBOTO_CSS_SHASH));

#[derive(Serialize)]
struct Page {
    title: String,
    //subtitle: String,
    body: String,
}

#[derive(Serialize)]
struct Context {
    config: &'static Config,
    about: Option<About>,
    resource: Option<Resource>,
    page: Option<Page>,
}

fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("header", HEADER).expect("Could not parse header template");
    tt.add_template("footer", FOOTER).expect("Could not parse footer template");
    tt.add_template("resource", RESOURCE).expect("Could not parse resource page template");
    tt.add_template("index", INDEX).expect("Could not parse index page template");
    tt.add_template("about", ABOUT).expect("Could not parse about page template");
    tt.add_template("custom", CUSTOM).expect("Could not parse about page template");
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

fn hash_etag<T: ?Sized>(r: &HttpRequest, body: &'static T, shash: &str, quoted: &str, ct: &str) -> impl Responder + use<T>
where &'static T: MessageBody {
    if let Some(e) = r.headers().get(header::IF_NONE_MATCH)
        && let Ok(s) = e.to_str()
        && s == quoted
    {
        return HttpResponse::NotModified().finish();
    }
    let tag = ETag(EntityTag::new_strong(shash.to_owned()));
    HttpResponse::Ok().content_type(ct).append_header((header::CACHE_CONTROL, "public, max-age=31536000, immutable")).append_header(tag).body(body)
}

// For maximum robustness, serve CSS, font and icon from any path. Collision with RDF resource URIs unlikely.
#[get("{_anypath:.*/|}rickview.css")]
async fn rickview_css(r: HttpRequest) -> impl Responder { hash_etag(&r, RICKVIEW_CSS, &RICKVIEW_CSS_SHASH, &RICKVIEW_CSS_SHASH_QUOTED, "text/css") }

#[get("{_anypath:.*/|}roboto.css")]
async fn roboto_css(r: HttpRequest) -> impl Responder { hash_etag(&r, ROBOTO_CSS, &ROBOTO_CSS_SHASH, &ROBOTO_CSS_SHASH_QUOTED, "text/css") }

// cached automatically by browser
#[get("{_anypath:.*/|}roboto300.woff2")]
async fn roboto300() -> impl Responder { HttpResponse::Ok().content_type("font/woff2").body(ROBOTO300) }

#[get("{_anypath:.*/|}favicon.ico")]
async fn favicon(r: HttpRequest) -> impl Responder { hash_etag(&r, &FAVICON[..], &FAVICON_SHASH, &FAVICON_SHASH_QUOTED, "image/x-icon") }

fn error_response(source: &str, error: impl std::fmt::Debug) -> HttpResponse {
    let message = format!("Could not render {source}: {error:?}");
    error!("{message}");
    HttpResponse::InternalServerError().body(message)
}

fn res_result(resource: &str, content_type: &str, result: Result<String, Box<dyn Error>>) -> HttpResponse {
    match result {
        Ok(s) => HttpResponse::Ok().content_type(content_type).body(s),
        Err(e) => error_response(&format!("resource {resource}"), e),
    }
}

// Pseudo GET parameters with empty value so that asset responders still match and caching works.
fn add_hashes(body: &str) -> String {
    body.replacen("rickview.css", &format!("rickview.css?{}", *RICKVIEW_CSS_SHASH), 1)
        .replacen("roboto.css", &format!("roboto.css?{}", *ROBOTO_CSS_SHASH), 1)
        .replacen("favicon.ico", &format!("favicon.ico?{}", *FAVICON_SHASH), 1)
}

#[derive(Deserialize)]
struct Params {
    output: Option<String>,
}

#[get("/{suffix:.*}")]
/// Serve an RDF resource either as HTML or one of various serializations depending on the accept header.
async fn rdf_resource(r: HttpRequest, suffix: web::Path<String>, params: web::Query<Params>) -> impl Responder {
    const NT: &str = "application/n-triples";
    const TTL: &str = "application/turtle";
    #[cfg(feature = "rdfxml")]
    const XML: &str = "application/rdf+xml";
    const HTML: &str = "text/html";
    let suffix: &str = &suffix;
    let id = RUN_ID.load(Ordering::Relaxed).to_string();
    let quoted = format!("\"{id}\"");
    if let Some(e) = r.headers().get(header::IF_NONE_MATCH)
        && let Ok(s) = e.to_str()
        && s == quoted
    {
        return HttpResponse::NotModified().finish();
    }
    let etag = ETag(EntityTag::new_strong(id));
    let output = params.output.as_deref();
    let t = Instant::now();
    let prefixed = config().prefix.to_string() + ":" + suffix;

    let iri = config().namespace.resolve(IriRef::new_unchecked(suffix));
    let mut res = rdf::resource(iri.as_ref());
    // no triples found
    if res.directs.is_empty() && res.inverses.is_empty() {
        // resource URI equal to namespace takes precedence
        if suffix.is_empty() {
            return index();
        }
        let warning = format!("No triples found for {suffix}. Did you configure the namespace correctly?");
        warn!("{warning}");
        if let Some(a) = r.head().headers().get("Accept")
            && let Ok(accept) = a.to_str()
            && accept.contains(HTML)
        {
            res.descriptions.push(("Warning".to_owned(), vec![warning.clone()]));
            // HTML is accepted and there are no errors, create a pseudo element in the empty resource to return 404 with HTML
            return match template().render("resource", &Context { config: config(), resource: Some(res), about: None, page: None }) {
                Ok(html) => HttpResponse::NotFound().content_type("text/html; charset-utf-8").append_header(etag).body(add_hashes(&html)),
                Err(e) => HttpResponse::NotFound().content_type("text/plain").append_header(etag).body(format!("{warning}\n\n{e}")),
            };
        }
        // return 404 with plain text
        return HttpResponse::NotFound().content_type("text/plain").append_header(etag).body(warning);
    }
    if let Some(a) = r.head().headers().get("Accept") {
        if let Ok(accept) = a.to_str() {
            trace!("{prefixed} accept header {accept}");
            if accept.contains(NT) || output == Some(NT) {
                debug!("{} N-Triples {:?}", prefixed, t.elapsed());
                return res_result(&prefixed, NT, rdf::serialize_nt(iri.as_ref()));
            }
            #[cfg(feature = "rdfxml")]
            if accept.contains(XML) || output == Some(XML) {
                debug!("{} RDF/XML {:?}", prefixed, t.elapsed());
                return res_result(&prefixed, XML, rdf::serialize_rdfxml(iri.as_ref()));
            }
            if accept.contains(HTML) && output != Some(TTL) {
                let context = Context { config: config(), about: None, page: None, resource: Some(res) };
                return match template().render("resource", &context) {
                    Ok(html) => {
                        debug!("{} HTML {:?}", prefixed, t.elapsed());
                        HttpResponse::Ok().content_type("text/html; charset-utf-8").append_header(etag).body(add_hashes(&html))
                    }
                    Err(err) => error_response(&format!("resource {prefixed}"), err),
                };
            }
            if !accept.contains(TTL) {
                warn!("{prefixed} accept header {accept} and 'output' param {output:?} not recognized, default to RDF Turtle");
            }
        }
    } else {
        warn!("{prefixed} accept header missing, using RDF Turtle");
    }
    debug!("{} RDF Turtle {:?}", prefixed, t.elapsed());
    res_result(&prefixed, TTL, rdf::serialize_turtle(iri.as_ref()))
}

/// does not get shown when there is a resource whose URI equals the namespace, with or without slash
fn index() -> HttpResponse {
    let context = Context { config: config(), about: None, page: None, resource: None };
    match template().render("index", &context) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(add_hashes(&body)),
        Err(e) => error_response("index page", e),
    }
}

#[get("/about")]
async fn about_page() -> impl Responder {
    let context = Context { config: config(), about: Some(About::new()), page: None, resource: None };
    match template().render("about", &context) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(add_hashes(&body)),
        Err(e) => error_response("about page", e),
    }
}

#[get("/classes")]
async fn class_page() -> impl Responder {
    let body = crate::classes::class_tree();
    let context = Context { config: config(), about: None, page: Some(Page { title: "Classes".to_owned(), body }), resource: None };
    match template().render("custom", &context) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(add_hashes(&body)),
        Err(e) => error_response("class page", e),
    }
}

#[head("{_anypath:.*}")]
async fn head() -> HttpResponse { HttpResponse::MethodNotAllowed().body("RickView does not support HEAD requests.") }

#[get("")]
/// redirect /base to correct index page /base/
/// For example, a user may erroneously open <http://mydomain.org/ontology> but mean <http://mydomain.org/ontology/>, which should be the base resource if it exists as the latter is inside the namespace.
async fn redirect() -> impl Responder { HttpResponse::TemporaryRedirect().append_header(("location", config().base.clone() + "/")).finish() }

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // we don't care about the upper bits as they rarely change
    #[allow(clippy::cast_possible_truncation)]
    RUN_ID.store(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u32, Ordering::Relaxed);
    config(); // enable logging
    info!("RickView {} serving {} at http://localhost:{}{}/", config::VERSION, config().namespace.as_str(), config().port, config().base);
    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .service(rickview_css)
            .service(roboto_css)
            .service(roboto300)
            .service(favicon)
            .service(head)
            .service(scope(&config().base).service(about_page).service(class_page).service(rdf_resource).service(redirect))
    })
    .bind(("0.0.0.0", config().port))?
    .run()
    .await
}
