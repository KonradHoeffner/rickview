#![warn(clippy::pedantic)]
#![allow(clippy::wildcard_imports)]
#![allow(clippy::enum_glob_use)]
#![allow(clippy::unused_async)]
#![allow(clippy::similar_names)]
#![feature(once_cell)]
//! Lightweight and performant RDF browser.
//! An RDF browser is a web application that *resolves* RDF resources: given the HTTP(s) URL identifying a resource it returns an HTML summary.
//! Besides HTML, the RDF serialization formats RDF/XML, Turtle and N-Triples are also available using content negotiation.
//! Default configuration is stored in `data/default.toml`, which can be overriden in `data/config.toml` or environment variables.
//! Configuration keys are in `lower\_snake\_ca`se, while environment variables are prefixed with RICKVIEW\_ and are `in SCREAMING\_SNAKE\`_CASE.
mod about;
/// The main module uses Actix Web to serve resources as HTML and other formats.
mod config;
mod rdf;
mod resource;

use crate::{config::config, resource::Resource, rdf::{namespace, Piri}};
use about::About;
use actix_web::{get, web, web::scope, App, HttpRequest, HttpResponse, HttpServer, Responder};
use log::{debug, error, info, trace, warn};
use std::time::Instant;
use tinytemplate::TinyTemplate;
use sophia::{term::SimpleIri, iri::Iri};

static RESOURCE: &str = std::include_str!("../data/resource.html");
static FAVICON: &[u8; 1150] = std::include_bytes!("../data/favicon-aksw.ico");
static CSS: &str = std::include_str!("../data/rickview.css");
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
async fn css() -> impl Responder { HttpResponse::Ok().content_type("text/css").body(CSS) }

#[get("{_anypath:.*/|}favicon.ico")]
async fn favicon() -> impl Responder { HttpResponse::Ok().content_type("image/x-icon").body(FAVICON.as_ref()) }

#[get("{suffix:.*|}")]
async fn res_html(request: HttpRequest, suffix: web::Path<String>) -> impl Responder {
    res_html_common(request, &namespace().get(&suffix).unwrap())
}

fn res_html_common(request: HttpRequest, resource: &SimpleIri<'_>) -> HttpResponse {
    let t = Instant::now();
    let local_suffix = resource.to_string().replace(&config().namespace, "");
    let prefixed = match Iri::new(&resource.to_string()) {
        Ok(iri) => Piri::new(iri.boxed()).short(),
        _ => resource.to_string()
    };
    match rdf::resource(resource) {
        Err(_) => {
            let message = format!("No triples found for resource {prefixed}");
            warn!("{}", message);
            match request.head().headers().get("Accept") {
                Some(a) => {
                    if let Ok(accept) = a.to_str() {
                        trace!("{} accept header {}", prefixed, accept);
                        if accept.contains("text/html") {
                            return match template().render("resource", &Resource {
                                suffix: local_suffix.clone(),
                                title: "404 Not found".to_string(),
                                title_maybe_link: "404 Not found".to_string(),
                                descriptions: vec![("Warning".to_string(), vec![message])],
                                directs: vec![],
                                inverses: vec![],
                                duration: "".to_string(),
                                uri: "".to_string(),
                                main_type: None,
                                github_issue_url: config().github.as_ref().map(|g| format!("{g}/issues/new?title={local_suffix}")),
                            }) {
                                Ok(html) => {
                                    debug!("{} HTML {:?}", prefixed, t.elapsed());
                                    HttpResponse::NotFound().content_type("text/html; charset-utf-8").body(html)
                                }
                                Err(err) => {
                                    let message = format!("Internal server error. Could not render resource {prefixed}:\n{err}.");
                                    error!("{}", message);
                                    HttpResponse::InternalServerError().body(message)
                                }
                            };
                        }
                    }
                }
                None => {}
            }
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
                            return HttpResponse::Ok().content_type("application/n-triples").body(rdf::serialize_nt(resource));
                        }
                        #[cfg(feature = "rdfxml")]
                        if accept.contains("application/rdf+xml") {
                            debug!("{} RDF {:?}", prefixed, t.elapsed());
                            return HttpResponse::Ok().content_type("application/rdf+xml").body(rdf::serialize_rdfxml(resource));
                        }
                        warn!("{} accept header {} not recognized, using RDF Turtle", prefixed, accept);
                    }
                }
                None => {
                    warn!("{} accept header missing, using RDF Turtle", prefixed);
                }
            }
            debug!("{} RDF Turtle {:?}", prefixed, t.elapsed());
            HttpResponse::Ok().content_type("application/turtle").body(rdf::serialize_turtle(resource))
        }
    }
}

#[get("/")]
async fn index(request: HttpRequest) -> impl Responder {
    index_impl(request)
}

fn index_impl(request: HttpRequest) -> HttpResponse {
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
    trace!("{:?}", config());
    info!("Serving {} at http://localhost:{}{}/", config().namespace, config().port, config().base);
    HttpServer::new(move || {
        App::new().service(css).service(favicon).service(scope(&config().base).service(index).service(about_page).service(redirect).service(res_html))
    })
    .bind(("0.0.0.0", config().port))?
    .run()
    .await
}
