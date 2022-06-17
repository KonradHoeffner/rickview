use serde::Serialize;

#[derive(Serialize)]
pub struct Resource {
    pub uri: String,
    pub suffix: String,
    pub directs: Vec<(String, Vec<String>)>,
    pub inverses: Vec<(String, Vec<String>)>,
    pub duration: String,
}
