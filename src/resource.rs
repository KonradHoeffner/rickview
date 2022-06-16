use serde::Serialize;

#[derive(Serialize)]
pub struct Resource {
    pub uri: String,
    pub suffix: String,
    pub directs: Vec<(String, String)>,
    pub duration: String,
}
