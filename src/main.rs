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
use actix_web::body::MessageBody;
use actix_web::http::StatusCode;
use actix_web::http::header::{self, ETag, EntityTag};
use actix_web::middleware::{Compress, DefaultHeaders};
use actix_web::web::scope;
use actix_web::{App, HttpRequest, HttpResponse, HttpServer, Responder, route, web};
use const_fnv1a_hash::{fnv1a_hash_32, fnv1a_hash_str_32};
use log::{info, warn};
use rickview::config::{self, config};
use rickview::render::{self, Format};
use serde::Deserialize;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
// extremely low risk of collision, worst case is out of date favicon or CSS
static FAVICON_HASH: u32 = fnv1a_hash_32(FAVICON, None);
static RICKVIEW_CSS: &str = std::include_str!("../data/rickview.css");
static RICKVIEW_CSS_HASH: u32 = fnv1a_hash_str_32(RICKVIEW_CSS);
static ROBOTO_CSS: &str = std::include_str!("../data/roboto.css");
static ROBOTO_CSS_HASH: u32 = fnv1a_hash_str_32(ROBOTO_CSS);
static ROBOTO300: &[u8] = std::include_bytes!("../fonts/roboto300.woff2");
static RUN_ID: AtomicU32 = AtomicU32::new(0);

// 8 chars hexadecimal, not worth it to add base64 dependency to save 2 chars
static FAVICON_SHASH: LazyLock<String> = LazyLock::new(|| format!("{FAVICON_HASH:x}"));
static FAVICON_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *FAVICON_SHASH));
static RICKVIEW_CSS_SHASH: LazyLock<String> = LazyLock::new(|| format!("{RICKVIEW_CSS_HASH:x}"));
static RICKVIEW_CSS_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *RICKVIEW_CSS_SHASH));
static ROBOTO_CSS_SHASH: LazyLock<String> = LazyLock::new(|| format!("{ROBOTO_CSS_HASH:x}"));
static ROBOTO_CSS_SHASH_QUOTED: LazyLock<String> = LazyLock::new(|| format!("\"{}\"", *ROBOTO_CSS_SHASH));

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

/// For maximum robustness, serve CSS, font and icon from any path. Collision with RDF resource URIs unlikely.
#[route("{_anypath:.*/|}rickview.css", method = "GET", method = "HEAD")]
async fn rickview_css(r: HttpRequest) -> impl Responder { hash_etag(&r, RICKVIEW_CSS, &RICKVIEW_CSS_SHASH, &RICKVIEW_CSS_SHASH_QUOTED, "text/css") }

#[route("{_anypath:.*/|}roboto.css", method = "GET", method = "HEAD")]
async fn roboto_css(r: HttpRequest) -> impl Responder { hash_etag(&r, ROBOTO_CSS, &ROBOTO_CSS_SHASH, &ROBOTO_CSS_SHASH_QUOTED, "text/css") }

/// cached automatically by browser
#[route("{_anypath:.*/|}roboto300.woff2", method = "GET", method = "HEAD")]
async fn roboto300() -> impl Responder { HttpResponse::Ok().content_type("font/woff2").body(ROBOTO300) }

#[route("{_anypath:.*/|}favicon.ico", method = "GET", method = "HEAD")]
async fn favicon(r: HttpRequest) -> impl Responder { hash_etag(&r, &FAVICON[..], &FAVICON_SHASH, &FAVICON_SHASH_QUOTED, "image/x-icon") }

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

fn html_response(output: render::RenderOutput, etag: Option<&str>) -> HttpResponse {
    let status = StatusCode::from_u16(output.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = if output.content_type == "text/html" { add_hashes(&output.body) } else { output.body };
    let mut response = HttpResponse::build(status).content_type(output.content_type).body(body);
    if let Some(tag) = etag
        && let Ok(value) = header::HeaderValue::from_str(&format!("\"{tag}\""))
    {
        response.headers_mut().append(header::ETAG, value);
    }
    response
}

#[route("/{suffix:.*}", method = "GET", method = "HEAD")]
/// Serve an RDF resource either as HTML or one of various serializations depending on the accept header.
async fn rdf_resource(r: HttpRequest, suffix: web::Path<String>, params: web::Query<Params>) -> impl Responder {
    const MIME_HTML: &str = "text/html";
    #[cfg(feature = "rdfxml")]
    const MIME_XML: &str = "application/rdf+xml";
    const MIME_NT: &str = "application/n-triples";
    const MIME_TTL: &str = "application/turtle";
    let suffix: &str = &suffix;
    let id = RUN_ID.load(Ordering::Relaxed).to_string();
    let quoted = format!("\"{id}\"");
    if let Some(e) = r.headers().get(header::IF_NONE_MATCH)
        && let Ok(s) = e.to_str()
        && s == quoted
    {
        return HttpResponse::NotModified().finish();
    }
    let output = params.output.as_deref();
    let prefixed = config().prefix.to_string() + ":" + suffix;

    let accept: Format = output
        .and_then(|o| match o {
            MIME_NT => Some(Format::Nt),
            MIME_TTL => Some(Format::Ttl),
            #[cfg(feature = "rdfxml")]
            MIME_XML => Some(Format::Xml),
            MIME_HTML => Some(Format::Html),
            _ => None,
        })
        .unwrap_or_else(|| {
            let a = r.head().headers().get("Accept").and_then(|a| a.to_str().ok());
            match a {
                Some(ah) if ah.contains(MIME_NT) => Format::Nt,
                Some(ah) if ah.contains(MIME_TTL) => Format::Ttl,
                #[cfg(feature = "rdfxml")]
                Some(ah) if ah.contains(MIME_XML) => Format::Xml,
                Some(ah) if ah.contains(MIME_HTML) => Format::Html,
                _ => {
                    warn!("{prefixed} accept header {a:?} and 'output' param missing or not recognized, default to RDF Turtle");
                    Format::Ttl
                }
            }
        });

    html_response(render::render_resource(suffix, accept), Some(&id))
}

#[route("/about", method = "GET", method = "HEAD")]
async fn about_page() -> impl Responder { html_response(render::render_about(), None) }

#[route("/config.json", method = "GET", method = "HEAD")]
async fn config_json() -> impl Responder { HttpResponse::Ok().json(config()) }

#[route("", method = "GET", method = "HEAD")]
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
            .wrap(DefaultHeaders::new().add((header::VARY, "accept")))
            .wrap(Compress::default())
            .service(rickview_css)
            .service(roboto_css)
            .service(roboto300)
            .service(favicon)
            .service(scope(&config().base).service(about_page).service(config_json).service(rdf_resource).service(redirect))
    })
    .bind(("0.0.0.0", config().port))?
    .run()
    .await
}
