#[macro_use]
extern crate lazy_static;
extern crate tinytemplate;

mod rdf;
mod resource;

use actix_web::{get, guard, web, App, HttpRequest, HttpResponse, HttpServer, Responder, Result};
use rdf::resource;
use resource::Resource;
use serde::Serialize;
use std::sync::Arc;
use tinytemplate::TinyTemplate;

static TEMPLATE: &str = std::include_str!("../data/template.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static CSS: &str = std::include_str!("../data/rickview.css");

#[get("/rickview.css")]
async fn css() -> impl Responder {
    HttpResponse::Ok().content_type("text/css").body(CSS)
}

#[get("/favicon.ico")]
async fn favicon() -> impl Responder {
    HttpResponse::Ok()
        .content_type("image/x-icon")
        .body(FAVICON.as_ref())
}

#[get("/ontology/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    let mut tt = TinyTemplate::new();
    tt.add_template("template", TEMPLATE).unwrap();
    let body = tt.render("template", &resource(&name)).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(css).service(favicon).service(greet))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
