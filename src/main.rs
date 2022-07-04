#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use crate::config::CONFIG;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use rdf::resource;
use std::fs;
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
    tt.add_template("resource", TEMPLATE)
        .expect("Could not parse default resource template");
    tt.add_template("index", INDEX)
        .expect("Could not parse default template");
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
    HttpResponse::Ok()
        .content_type("image/x-icon")
        .body(FAVICON.as_ref())
}

#[get("{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    let body = template().render("resource", &resource(&name)).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
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
                .service(greet)
                .service(index), //.route("/", web::get().to(HetpResponse::Ok().content_type("text/html").body(index_body)))
                                 //.route("/", web::get().to(index_responder))
                                 //.service(index(index_body)),
        )
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
