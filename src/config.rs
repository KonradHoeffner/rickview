use serde::Deserialize;

#[derive(Deserialize, Debug)]
pub struct Config {
    pub kb_file: String,
    pub namespace: String, 
    //pub type_properties: Vec<String>,
    //pub description_properties: Vec<String>
}

lazy_static! {
    pub static ref CONFIG: Config = envy::prefixed("RICKVIEW_")
        .from_env::<Config>()
        .expect("Could not read environment variables.");
}
