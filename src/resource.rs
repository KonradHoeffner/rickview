use serde::Serialize;

/// Summary of an RDF resource.
#[derive(Serialize)]
pub struct Resource {
    pub uri: String,
    pub base: String,
    //pub suffix: String,
    pub title: String,
    pub main_type: Option<String>,
    /// HTML representations of properties and descriptions of this resource.
    pub descriptions: Vec<(String, Vec<String>)>,
    /// HTML representations of properties and objects of triples where this resource is a subject.
    pub directs: Vec<(String, Vec<String>)>,
    /// HTML representations of subjects and properties of triples where this resource is an object.
    pub inverses: Vec<(String, Vec<String>)>,
    pub duration: String,
    pub github_issue_url: Option<String>,
    pub depiction: Option<String>,
}
