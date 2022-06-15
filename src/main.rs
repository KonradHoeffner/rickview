#[macro_use]
extern crate lazy_static;

mod page;

use actix_files::NamedFile;
use actix_web::{get, web, App, HttpRequest, HttpResponse, HttpServer, Responder, Result};
use page::page;

async fn favicon(_req: HttpRequest) -> Result<NamedFile> {
    Ok(NamedFile::open("favicon.ico")?)
}

async fn css(_req: HttpRequest) -> Result<NamedFile> {
    Ok(NamedFile::open("rickview.css")?)
}

#[get("/ontology/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    HttpResponse::Ok()
        .content_type("text/html")
        .body(page(name.as_ref()))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(greet)
            .route("/rickview.css", web::get().to(css))
            .route("/favicon.ico", web::get().to(favicon))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
