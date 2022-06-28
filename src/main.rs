#[macro_use]
extern crate lazy_static;

mod config;
mod rdf;
mod resource;

use actix_web::{get, web, App, HttpResponse, HttpServer, Responder};
use rdf::resource;
use tinytemplate::{TinyTemplate, ValueFormatter};

static TEMPLATE: &str = std::include_str!("../data/template.html");
static FAVICON: &[u8; 318] = std::include_bytes!("../data/favicon.ico");
static CSS: &str = std::include_str!("../data/rickview.css");

#[get("/ontology/rickview.css")]
async fn css() -> impl Responder {
    HttpResponse::Ok().content_type("text/css").body(CSS)
}

#[get("/ontology/favicon.ico")]
async fn favicon() -> impl Responder {
    HttpResponse::Ok()
        .content_type("image/x-icon")
        .body(FAVICON.as_ref())
}

#[get("/ontology/{name}")]
async fn greet(name: web::Path<String>) -> impl Responder {
    let mut tt = TinyTemplate::new();
    tt.add_template("template", TEMPLATE).unwrap();
    tt.add_formatter("suffix", |v, output| {
        let o = || -> Option<String> {
            let mut s = v.as_str().unwrap().rsplit_once("/").unwrap().1;
            if s.contains('#') {s = s.rsplit_once("#").unwrap().1;}
            Some(s.to_owned())
        };
        output.push_str(&o().unwrap());
        Ok(())
    });
    /*
    tt.add_formatter("title", |v, output| {
        let o  = || -> Option<String> {Some(v.get(0)?.get(1)?.get(0)?.to_string().split("@").next()?[1..].to_owned())};
        output.push_str(&o().unwrap_or("label not found".to_owned()));
        Ok(())
    });
    */
    let body = tt.render("template", &resource(&name)).unwrap();
    HttpResponse::Ok().content_type("text/html").body(body)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    //dotenv().ok();
    HttpServer::new(|| App::new().service(css).service(favicon).service(greet))
        .bind(("0.0.0.0", 8080))?
        .run()
        .await
}
