//! Lightweight and performant RDF browser.
//! An RDF browser is a web application that *resolves* RDF resources: given the HTTP(s) URL identifying a resource it returns an HTML summary.
//! Besides HTML, the RDF serialization formats RDF/XML, Turtle and N-Triples are also available using content negotiation.
//! Default configuration is stored in `data/default.toml`, which can be overriden in `data/config.toml` or environment variables.
//! Configuration keys are in lower\_snake\_case, while environment variables are prefixed with RICKVIEW\_ and are in SCREAMING\_SNAKE\_CASE.
/// The main module uses Actix Web to serve resources as HTML and other formats.
#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use crate::config::CONFIG;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use log::{debug, error, info, trace, warn};
use tinytemplate::TinyTemplate;

static TEMPLATE: &str = std::include_str!("../data/template.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static CSS: &str = std::include_str!("../data/rickview.css");
static INDEX: &str = std::include_str!("../data/index.html");

fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("resource", TEMPLATE).expect("Could not parse default resource template");
    tt.add_template("index", INDEX).expect("Could not parse default template");
    tt.add_formatter("uri_to_suffix", |json, output| {
        let o = || -> Option<String> {
            let s = json.as_str().unwrap_or_else(|| panic!("JSON value is not a string: {}", json));
            let mut s = s.rsplit_once('/').unwrap_or_else(|| panic!("no '/' in URI '{}'", s)).1;
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
async fn resource_html(request: HttpRequest, suffix: web::Path<String>) -> impl Responder {
    let prefixed = CONFIG.prefix.clone() + ":" + &suffix;
    match rdf::resource(&suffix) {
        Err(_) => {
            let message = format!("No triples found for resource {}", prefixed);
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
                                    debug!("{} serve as HTML", prefixed);
                                    HttpResponse::Ok().content_type("text/html; charset-utf-8").body(html)
                                }
                                Err(err) => {
                                    let message = format!("Internal server error. Could not render resource {}:\n{}.", prefixed,err);
                                    error!("{}", message);
                                    HttpResponse::InternalServerError().body(message)
                                }
                            };
                        }
                        if accept.contains("application/n-triples") {
                            debug!("{} serve as as N-Triples", prefixed);
                            return HttpResponse::Ok().content_type("application/n-triples").body(rdf::serialize_nt(&suffix));
                        }
                        #[cfg(feature = "rdfxml")]
                        if accept.contains("application/rdf+xml") {
                            debug!("{} serve as RDF", prefixed);
                            return HttpResponse::Ok().content_type("application/rdf+xml").body(rdf::serialize_rdfxml(&suffix));
                        }
                        warn!("{} accept header {} not recognized, using default", prefixed, accept);
                    }
                }
                None => {
                    warn!("{} accept header missing, using default", prefixed);
                }
            }
            debug!("{} serve as RDF Turtle", prefixed);
            HttpResponse::Ok().content_type("application/turtle").body(rdf::serialize_turtle(&suffix))
        }
    }
}

#[get("/")]
async fn index() -> impl Responder {
    match template().render("index", &*CONFIG) {
        Ok(body) => HttpResponse::Ok().content_type("text/html").body(body),
        Err(e) => {
            let message = format!("Could not render index page: {:?}", e);
            error!("{}", message);
            HttpResponse::InternalServerError().body(message)
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    #[cfg(feature = "log")]
    {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", format!("rickview={}", CONFIG.log_level.as_ref().unwrap_or(&"info".to_owned())));
        }
        env_logger::builder().format_timestamp(None).format_target(false).init();
    }
    trace!("{:?}", &*CONFIG);
    let server =
        HttpServer::new(move || App::new().service(web::scope(&CONFIG.base_path).service(css).service(favicon).service(index).service(resource_html)))
            .bind(("0.0.0.0", CONFIG.port))?
            .run();
    info!("Serving {} at http://localhost:{}{}", CONFIG.namespace, CONFIG.port, CONFIG.base_path);
    server.await
}
