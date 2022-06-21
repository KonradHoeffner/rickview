use serde::Serialize;

#[derive(Serialize)]
pub struct Property {
    pub uri: String,
    pub tooltip: String,
}

#[derive(Serialize)]
pub struct Resource {
    pub uri: String,
    pub suffix: String,
    pub descriptions: Vec<(String, Vec<String>)>,
    pub directs: Vec<(String, Vec<String>)>,
    pub inverses: Vec<(String, Vec<String>)>,
    pub duration: String,
}
