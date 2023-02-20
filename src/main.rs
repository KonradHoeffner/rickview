#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::similar_names)]
#![deny(rust_2018_idioms)]
#![feature(once_cell)]
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
use actix_web::middleware::Compress;
use actix_web::{get, web, web::scope, App, HttpRequest, HttpResponse, HttpServer, Responder};
use log::{debug, error, info, trace, warn};
use std::time::Instant;
use tinytemplate::TinyTemplate;

static RESOURCE: &str = std::include_str!("../data/resource.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static RICKVIEW_CSS: &str = std::include_str!("../data/rickview.css");
static ROBOTO_CSS: &str = std::include_str!("../data/roboto.css");
static ROBOTO300: &[u8] = std::include_bytes!("../fonts/roboto300.woff2");
static INDEX: &str = std::include_str!("../data/index.html");
static ABOUT: &str = std::include_str!("../data/about.html");

fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("resource", RESOURCE).expect("Could not parse resource page template");
    tt.add_template("index", INDEX).expect("Could not parse index page template");
    tt.add_template("about", ABOUT).expect("Could not parse about page template");
    tt.add_formatter("uri_to_suffix", |json, output| {
        let o = || -> Option<String> {
            let s = json.as_str().unwrap_or_else(|| panic!("JSON value is not a string: {json}"));
            let mut s = s.rsplit_once('/').unwrap_or_else(|| panic!("no '/' in URI '{s}'")).1;
            if s.contains('#') {
                s = s.rsplit_once('#')?.1;
            }
            Some(s.to_owned())
        };
        output.push_str(&o().unwrap());
        Ok(())
    });
    tt
}

#[get("{_anypath:.*/|}rickview.css")]
async fn rickview_css() -> impl Responder { HttpResponse::Ok().content_type("text/css").body(RICKVIEW_CSS) }

#[get("{_anypath:.*/|}roboto.css")]
async fn roboto_css() -> impl Responder { HttpResponse::Ok().content_type("text/css").body(ROBOTO_CSS) }

#[get("{_anypath:.*/|}roboto300.woff2")]
async fn roboto300() -> impl Responder { HttpResponse::Ok().content_type("font/woff2").body(ROBOTO300) }

#[get("{_anypath:.*/|}favicon.ico")]
async fn favicon() -> impl Responder { HttpResponse::Ok().content_type("image/x-icon").body(FAVICON.as_ref()) }

#[get("{suffix:.*|}")]
async fn res_html(request: HttpRequest, suffix: web::Path<String>) -> impl Responder {
    let t = Instant::now();
    let prefixed = config().prefix.to_string() + ":" + &suffix;
    match rdf::resource(&suffix) {
        Err(_) => {
            let message = format!("No triples found for resource {prefixed}");
            warn!("{}", message);
            HttpResponse::NotFound().content_type("text/plain").body(message)
        }
        Ok(res) => {
            match request.head().headers().get("Accept") {
                Some(a) => {
                    if let Ok(accept) = a.to_str() {
                        trace!("{} accept header {}", prefixed, accept);
                        if accept.contains("text/html") {
                            return match template().render("resource", &res) {
                                Ok(html) => {
                                    debug!("{} HTML {:?}", prefixed, t.elapsed());
                                    HttpResponse::Ok().content_type("text/html; charset-utf-8").body(html)
                                }
                                Err(err) => {
                                    let message = format!("Internal server error. Could not render resource {prefixed}:\n{err}.");
                                    error!("{}", message);
                                    HttpResponse::InternalServerError().body(message)
                                }
                            };
                        }
                        if accept.contains("application/n-triples") {
                            debug!("{} N-Triples {:?}", prefixed, t.elapsed());
                            return HttpResponse::Ok().content_type("application/n-triples").body(rdf::serialize_nt(&suffix));
                        }
                        #[cfg(feature = "rdfxml")]
                        if accept.contains("application/rdf+xml") {
                            debug!("{} RDF {:?}", prefixed, t.elapsed());
                            return HttpResponse::Ok().content_type("application/rdf+xml").body(rdf::serialize_rdfxml(&suffix));
                        }
                        warn!("{} accept header {} not recognized, using RDF Turtle", prefixed, accept);
                    }
                }
                None => {
                    warn!("{} accept header missing, using RDF Turtle", prefixed);
                }
            }
            debug!("{} RDF Turtle {:?}", prefixed, t.elapsed());
            HttpResponse::Ok().content_type("application/turtle").body(rdf::serialize_turtle(&suffix))
        }
    }
}

#[get("/")]
async fn index() -> impl Responder {
    match template().render("index", config()) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(body),
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
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(body),
        Err(e) => {
            let message = format!("Could not render about page: {e:?}");
            error!("{}", message);
            HttpResponse::InternalServerError().body(message)
        }
    }
}

// redirect /base to correct index page /base/
#[get("")]
async fn redirect() -> impl Responder { HttpResponse::TemporaryRedirect().append_header(("location", config().base.clone() + "/")).finish() }

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let _ = config(); // needed to enable logging
    info!("RickView {} serving {} at http://localhost:{}{}/", config::VERSION, config().namespace, config().port, config().base);
    HttpServer::new(move || {
        App::new()
            .wrap(Compress::default())
            .service(rickview_css)
            .service(roboto_css)
            .service(roboto300)
            .service(favicon)
            .service(scope(&config().base).service(index).service(about_page).service(redirect).service(res_html))
    })
    .bind(("0.0.0.0", config().port))?
    .run()
    .await
}
