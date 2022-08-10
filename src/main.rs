#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use crate::config::CONFIG;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
//use std::fs;
use log::{debug, error, info, trace, warn};
use tinytemplate::TinyTemplate;

static TEMPLATE: &str = std::include_str!("../data/template.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static CSS: &str = std::include_str!("../data/rickview.css");
static INDEX: &str = std::include_str!("../data/index.html");

lazy_static! {
/*    static ref INDEX_BODY: String = fs::read_to_string(&CONFIG.index_file.as_ref().unwrap())
        .expect(&format!(
            "Unable to load index file {}",
            &CONFIG.index_file.as_ref().unwrap()
        ));*/
}

// TODO: reuse existing template
fn template() -> TinyTemplate<'static> {
    let mut tt = TinyTemplate::new();
    tt.add_template("resource", TEMPLATE).expect("Could not parse default resource template");
    tt.add_template("index", INDEX).expect("Could not parse default template");
    /*
    match &CONFIG.template_file {
        None => {
            tt.add_template("template", TEMPLATE)
                .expect("Could not parse default template");
        }
        Some(path) => {
            tt.add_template("template",&fs::read_to_string(path).expect(&format!("Could not read template file {}", path)))
                .expect(&format!("Could not add custom template file {}", path));
        }
    };
    */
    tt.add_formatter("suffix", |v, output| {
        let o = || -> Option<String> {
            let mut s = v.as_str().unwrap().rsplit_once('/').unwrap().1;
            if s.contains('#') {
                s = s.rsplit_once('#').unwrap().1;
            }
            Some(s.to_owned())
        };
        output.push_str(&o().unwrap());
        Ok(())
    });
    tt
}

#[get("rickview.css")]
async fn css() -> impl Responder {
    HttpResponse::Ok().content_type("text/css").body(CSS)
}

#[get("favicon.ico")]
async fn favicon() -> impl Responder {
    HttpResponse::Ok().content_type("image/x-icon").body(FAVICON.as_ref())
}

#[get("{suffix}")]
async fn resource_html(request: HttpRequest, suffix: web::Path<String>) -> impl Responder {
    let prefixed = CONFIG.prefix.clone() + ":" + &suffix;
    match rdf::resource(&suffix) {
        None => {
            let message = format!("No triples found for resource {}", prefixed);
            warn!("{}", message);
            HttpResponse::NotFound().content_type("text/plain").body(message)
        }
        Some(res) => {
            match request.head().headers().get("Accept") {
                Some(a) => {
                    if let Ok(accept) = a.to_str() {
                        trace!("{} accept header {}", prefixed, accept);
                        if accept.contains("text/html") {
                            return match template().render("resource", &res) {
                                Ok(html) => {
                                    debug!("{} serve as HTML", prefixed);
                                    HttpResponse::Ok().content_type("text/html").body(html)
                                }
                                Err(_) => {
                                    let message = format!("Internal server error. Could not render resource {}.", prefixed);
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
    let body = template().render("index", &*CONFIG).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
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
    /*    let index_body = fs::read_to_string(&CONFIG.index_file.as_ref().unwrap());
    let response = HttpResponse::Ok().content_type("text/html");
    let index_responder = || response;*/
    trace!("{:?}", &*CONFIG);
    let server = HttpServer::new(move || {
        App::new().service(
            web::scope(&CONFIG.base_path)
                .service(css)
                .service(favicon)
                .service(resource_html)
                .service(index),
        )
    })
    .bind(("0.0.0.0", CONFIG.port))?
    .run();
    info!("Serving {} at http://0.0.0.0:{}", CONFIG.namespace, CONFIG.port);
    //log::info!("{} triples loaded from {}", graph.triples().count() , &CONFIG.kb_file );
    server.await
}
