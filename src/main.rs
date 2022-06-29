#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use crate::config::CONFIG;
use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use rdf::resource;
use tinytemplate::TinyTemplate;

static TEMPLATE: &str = std::include_str!("../data/template.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static CSS: &str = std::include_str!("../data/rickview.css");

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
    let mut tt = TinyTemplate::new();
    tt.add_template("template", TEMPLATE).unwrap();
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
    let body = tt.render("template", &resource(&name)).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    //dotenv().ok();
    HttpServer::new(|| {
        App::new().service(
            web::scope(&CONFIG.base_path)
                .service(css)
                .service(favicon)
                .service(greet),
        )
    })
    .bind(("0.0.0.0", 8080))?
    .run()
    .await
}
