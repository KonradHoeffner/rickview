//! WebAssembly entry point for the client-side HDT viewer.
use rickview::config::{self, Config};
use rickview::render::{self, Format, RenderOutput};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
}

/// Configure the viewer from a TOML string. Must be called before the first render.
#[wasm_bindgen]
pub fn configure(toml: &str) -> Result<(), JsValue> {
    let config = Config::from_toml(toml).map_err(|e| JsValue::from_str(&e.to_string()))?;
    config::set(config).map_err(|_| JsValue::from_str("Configuration already initialized"))?;
    Ok(())
}

/// Load an HDT or HDT.zst file. Only one graph can be loaded per page.
#[wasm_bindgen]
pub fn load_hdt(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    rickview::rdf::load_hdt(name, bytes.to_vec()).map_err(|e| JsValue::from_str(&format!("{e:?}")))
}

fn output(value: RenderOutput) -> Result<JsValue, JsValue> {
    let object = js_sys::Object::new();
    js_sys::Reflect::set(&object, &"status".into(), &JsValue::from_f64(f64::from(value.status)))?;
    js_sys::Reflect::set(&object, &"contentType".into(), &JsValue::from_str(value.content_type))?;
    js_sys::Reflect::set(&object, &"body".into(), &JsValue::from_str(&value.body))?;
    js_sys::Reflect::set(&object, &"fragment".into(), &JsValue::from_str(&value.fragment))?;
    Ok(object.into())
}

/// Render a resource by its suffix, either as HTML or as N-Triples/Turtle.
#[wasm_bindgen]
pub fn render(suffix: &str, format: &str) -> Result<JsValue, JsValue> {
    let format = match format {
        "html" => Format::Html,
        "nt" => Format::Nt,
        "ttl" => Format::Ttl,
        _ => return Err(JsValue::from_str("format must be html, nt or ttl")),
    };
    output(render::render_resource(suffix, format))
}

#[wasm_bindgen]
pub fn render_about() -> Result<JsValue, JsValue> { output(render::render_about()) }
