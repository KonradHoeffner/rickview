mod page;

use actix_web::{get, web, App, HttpServer, Responder};
use page::page;

#[get("/hello/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    page("")
}

#[actix_web::main] // or #[tokio::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new().service(greet)
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
