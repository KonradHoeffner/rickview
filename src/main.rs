#[macro_use]
extern crate lazy_static;

mod page;

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use page::page;

#[get("/ontology/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(page(name.as_ref()))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| App::new().service(greet))
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
