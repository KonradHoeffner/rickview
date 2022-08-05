#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use crate::config::CONFIG;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
//use std::fs;
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
            let mut s = v.as_str().unwrap().rsplit_once("/").unwrap().1;
            if s.contains('#') {
                s = s.rsplit_once("#").unwrap().1;
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
    match rdf::resource(&suffix) {
        None => HttpResponse::NotFound()
            .content_type("text/plain")
            .body(format!("No triples found for resource {}", suffix.to_owned())),
        Some(res) => {
            if let Some(a) = request.head().headers().get("Accept") {
                if let Ok(accept) = a.to_str() {
                    //println!("{accept}");
                    if accept.contains("text/html") {
                        return match template().render("resource", &res) {
                            Ok(html) => HttpResponse::Ok().content_type("text/html").body(html),
                            Err(_) => HttpResponse::InternalServerError()
                                .body(format!("Internal server error. Could not render resource {}.", suffix.to_owned())),
                        };
                    }
                    if accept.contains("application/n-triples") {
                        return HttpResponse::Ok().content_type("application/n-triples").body(rdf::serialize_nt(&suffix));
                    }
                    #[cfg(feature = "rdfxml")]
                    if accept.contains("application/rdf+xml") {
                        return HttpResponse::Ok().content_type("application/rdf+xml").body(rdf::serialize_rdfxml(&suffix));
                    }
                }
            }
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
    /*    let index_body = fs::read_to_string(&CONFIG.index_file.as_ref().unwrap());
    let response = HttpResponse::Ok().content_type("text/html");
    let index_responder = || response;*/
    HttpServer::new(move || {
        App::new().service(
            web::scope(&CONFIG.base_path)
                .service(css)
                .service(favicon)
                .service(resource_html)
                /*.service(
                    web::resource("{suffix}")
                        .guard(guard::fn_guard(|ctx| {
                            ctx.head().headers().get_all("Accept")
                        }))
                        .route(web::get().to(resource_html)),
                )*/
                .service(index),
        )
    })
    .bind(("0.0.0.0", CONFIG.port))?
    .run()
    .await
}
