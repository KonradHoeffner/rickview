#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::similar_names)]
#![deny(rust_2018_idioms)]
#![feature(once_cell)]
#![feature(let_chains)]
//! Lightweight and performant RDF browser.
//! An RDF browser is a web application that *resolves* RDF resources: given the HTTP(s) URL identifying a resource it returns an HTML summary.
//! Besides HTML, the RDF serialization formats RDF/XML, Turtle and N-Triples are also available using content negotiation.
//! Default configuration is stored in `data/default.toml`, which can be overriden in `data/config.toml` or environment variables.
//! Configuration keys are in `lower_snake_case`, while environment variables are prefixed with RICKVIEW\_ and are `in SCREAMING_SNAKE_CASE`.
mod about;
/// The main module uses Actix Web to serve resources as HTML and other formats.
mod config;
mod rdf;
mod resource;

use crate::config::config;
use about::About;
use actix_web::http::header::{self, ETag, EntityTag};
use actix_web::middleware::Compress;
use actix_web::web::scope;
use actix_web::{get, head, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use const_fnv1a_hash::fnv1a_hash_str_32;
use log::{debug, error, info, trace, warn};
use serde::Deserialize;
use std::error::Error;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tinytemplate::TinyTemplate;
#[macro_use]
extern crate lazy_static;

static RESOURCE: &str = std::include_str!("../data/resource.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static RICKVIEW_CSS: &str = std::include_str!("../data/rickview.css");
// extremely low risk of collision, worst case is out of date CSS
static RICKVIEW_CSS_HASH: u32 = fnv1a_hash_str_32(RICKVIEW_CSS);
static ROBOTO_CSS: &str = std::include_str!("../data/roboto.css");
static ROBOTO_CSS_HASH: u32 = fnv1a_hash_str_32(ROBOTO_CSS);
static ROBOTO300: &[u8] = std::include_bytes!("../fonts/roboto300.woff2");
static INDEX: &str = std::include_str!("../data/index.html");
static ABOUT: &str = std::include_str!("../data/about.html");
static RUN_ID: AtomicU32 = AtomicU32::new(0);

lazy_static! {
    // 8 chars hexadecimal, not worth it to add base64 dependency to save 2 chars
    static ref RICKVIEW_CSS_SHASH: String = format!("{RICKVIEW_CSS_HASH:x}");
    static ref RICKVIEW_CSS_SHASH_QUOTED: String = format!("\"{}\"",*RICKVIEW_CSS_SHASH);
    static ref ROBOTO_CSS_SHASH: String = format!("{ROBOTO_CSS_HASH:x}");
    static ref ROBOTO_CSS_SHASH_QUOTED: String = format!("\"{}\"",*ROBOTO_CSS_SHASH);
}

fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("resource", RESOURCE).expect("Could not parse resource page template");
    tt.add_template("index", INDEX).expect("Could not parse index page template");
    tt.add_template("about", ABOUT).expect("Could not parse about page template");
    tt.add_formatter("uri_to_suffix", |json, output| {
        let o = || -> String {
            let s = json.as_str().unwrap_or_else(|| panic!("JSON value is not a string: {json}"));
            let mut s = s.rsplit_once('/').unwrap_or_else(|| panic!("no '/' in URI '{s}'")).1;
            if s.contains('#') {
                s = s.rsplit_once('#').unwrap().1;
            }
            s.to_owned()
        };
        output.push_str(&o());
        Ok(())
    });
    tt
}

fn hash_etag(r: &HttpRequest, body: &'static str, shash: &str, quoted: &str, ct: &str) -> impl Responder {
    if let Some(e) = r.headers().get(header::IF_NONE_MATCH) {
        if let Ok(s) = e.to_str() {
            if s == quoted {
                return HttpResponse::NotModified().finish();
            }
        }
    }
    let tag = ETag(EntityTag::new_strong(shash.to_owned()));
    HttpResponse::Ok().content_type(ct).append_header((header::CACHE_CONTROL, "public, max-age=31536000, immutable")).append_header(tag).body(body)
}

#[get("{_anypath:.*/|}rickview.css")]
async fn rickview_css(r: HttpRequest) -> impl Responder { hash_etag(&r, RICKVIEW_CSS, &RICKVIEW_CSS_SHASH, &RICKVIEW_CSS_SHASH_QUOTED, "text/css") }

#[get("{_anypath:.*/|}roboto.css")]
async fn roboto_css(r: HttpRequest) -> impl Responder { hash_etag(&r, ROBOTO_CSS, &ROBOTO_CSS_SHASH, &ROBOTO_CSS_SHASH_QUOTED, "text/css") }

// cached automatically by browser
#[get("{_anypath:.*/|}roboto300.woff2")]
async fn roboto300() -> impl Responder { HttpResponse::Ok().content_type("font/woff2").body(ROBOTO300) }

#[get("{_anypath:.*/|}favicon.ico")]
async fn favicon() -> impl Responder { HttpResponse::Ok().content_type("image/x-icon").body(FAVICON.as_ref()) }

fn res_result(resource: &str, content_type: &str, result: Result<String, Box<dyn Error>>) -> HttpResponse {
    match result {
        Ok(s) => HttpResponse::Ok().content_type(content_type).body(s),
        Err(err) => {
            let message = format!("Internal server error. Could not render resource {resource}:\n{err}.");
            error!("{}", message);
            HttpResponse::InternalServerError().body(message)
        }
    }
}

fn add_hashes(body: &str) -> String {
    body.replacen("rickview.css", &format!("rickview.css?{}", *RICKVIEW_CSS_SHASH), 1).replacen(
        "roboto.css",
        &format!("roboto.css?{}", *ROBOTO_CSS_SHASH),
        1,
    )
}

#[derive(Deserialize)]
struct Params {
    output: Option<String>,
}

#[get("{suffix:.*|}")]
async fn res_html(r: HttpRequest, suffix: web::Path<String>, params: web::Query<Params>) -> impl Responder {
    const NT: &str = "application/n-triples";
    const TTL: &str = "application/turtle";
    const XML: &str = "application/rdf+xml";
    const HTML: &str = "text/html";
    let id = RUN_ID.load(Ordering::Relaxed).to_string();
    let quoted = format!("\"{id}\"");
    if let Some(e) = r.headers().get(header::IF_NONE_MATCH) {
        if let Ok(s) = e.to_str() {
            if s == quoted {
                return HttpResponse::NotModified().finish();
            }
        }
    }
    let etag = ETag(EntityTag::new_strong(id));
    let output = params.output.as_deref();
    let t = Instant::now();
    let prefixed = config().prefix.to_string() + ":" + &suffix;
    match rdf::resource(&suffix) {
        // TODO: eliminate this case by converting suffix to IRI in this method
        Err(_) => {
            let message = format!("No triples found for resource {prefixed}");
            warn!("{}", message);
            HttpResponse::NotFound().content_type("text/plain").append_header(etag).body(message)
        }
        // TODO: simplify and integrate with the main match case
        // no triples found
        Ok(mut res) if res.directs.is_empty() && res.inverses.is_empty() => {
            let warning = format!("No triples found for {suffix}. Did you configure the namespace correctly?");
            warn!("{}", warning);
            if let Some(a) = r.head().headers().get("Accept") {
                if let Ok(accept) = a.to_str() {
                    if accept.contains(HTML) {
                        res.descriptions.push(("Warning".to_owned(), vec![warning.clone()]));
                        // HTML is accepted and there are no errors, create a pseudo element in the empty resource to return 404 with HTML
                        if let Ok(html) = template().render("resource", &res) {
                            return HttpResponse::NotFound().content_type("text/html; charset-utf-8").append_header(etag).body(add_hashes(&html));
                        }
                    }
                }
            }
            // return 404 with plain text
            HttpResponse::NotFound().content_type("text/plain").append_header(etag).body(warning)
        }
        Ok(res) => {
            if let Some(a) = r.head().headers().get("Accept") {
                if let Ok(accept) = a.to_str() {
                    trace!("{} accept header {}", prefixed, accept);
                    if accept.contains(NT) || output == Some(NT) {
                        debug!("{} N-Triples {:?}", prefixed, t.elapsed());
                        return res_result(&prefixed, NT, rdf::serialize_nt(&suffix));
                    }
                    #[cfg(feature = "rdfxml")]
                    if accept.contains(XML) || output == Some(XML) {
                        debug!("{} RDF/XML {:?}", prefixed, t.elapsed());
                        return res_result(&prefixed, XML, rdf::serialize_rdfxml(&suffix));
                    }
                    if accept.contains(HTML) && output != Some(TTL) {
                        return match template().render("resource", &res) {
                            Ok(html) => {
                                debug!("{} HTML {:?}", prefixed, t.elapsed());
                                HttpResponse::Ok().content_type("text/html; charset-utf-8").append_header(etag).body(add_hashes(&html))
                            }
                            Err(err) => {
                                let message = format!("Internal server error. Could not render resource {prefixed}:\n{err}.");
                                error!("{}", message);
                                HttpResponse::InternalServerError().append_header(etag).body(message)
                            }
                        };
                    }
                    warn!("{} accept header {} and 'output' param {:?} not recognized or Turtle, using RDF Turtle", prefixed, accept, output);
                }
            } else {
                warn!("{} accept header missing, using RDF Turtle", prefixed);
            }
            debug!("{} RDF Turtle {:?}", prefixed, t.elapsed());
            res_result(&prefixed, TTL, rdf::serialize_turtle(&suffix))
        }
    }
}

#[get("/")]
async fn index() -> impl Responder {
    match template().render("index", config()) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(add_hashes(&body)),
        Err(e) => {
            let message = format!("Could not render index page: {e:?}");
            error!("{}", message);
            HttpResponse::InternalServerError().body(message)
        }
    }
}

#[get("/about")]
async fn about_page() -> impl Responder {
    match template().render("about", &About::new()) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(add_hashes(&body)),
        Err(e) => {
            let message = format!("Could not render about page: {e:?}");
            error!("{}", message);
            HttpResponse::InternalServerError().body(message)
        }
    }
}

#[head("{_anypath:.*}")]
async fn head() -> HttpResponse { HttpResponse::MethodNotAllowed().body("RickView does not support HEAD requests.") }

// redirect /base to correct index page /base/
#[get("")]
async fn redirect() -> impl Responder { HttpResponse::TemporaryRedirect().append_header(("location", config().base.clone() + "/")).finish() }

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // we don't care about the upper bits as they rarely change
    #[allow(clippy::cast_possible_truncation)]
    RUN_ID.store(SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u32, Ordering::Relaxed);
    config(); // enable logging
    info!("RickView {} serving {} at http://localhost:{}{}/", config::VERSION, config().namespace, config().port, config().base);
    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .service(rickview_css)
            .service(roboto_css)
            .service(roboto300)
            .service(favicon)
            .service(head)
            .service(scope(&config().base).service(index).service(about_page).service(redirect).service(res_html))
    })
    .bind(("0.0.0.0", config().port))?
    .run()
    .await
}
