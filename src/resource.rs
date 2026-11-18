use serde::Serialize;

/// A group of OWL axioms shown together, for example all superclasses of a class.
#[derive(Serialize, Debug, Clone)]
pub struct OntologySection {
    pub label: String,
    pub entries: Vec<String>,
}

/// Syntax used to display OWL axioms.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OwlSyntax {
    /// OWL 2 Manchester syntax.
    #[default]
    Manchester,
    /// OWL 2 functional-style syntax.
    Functional,
}

impl OwlSyntax {
    /// Parse a syntax name, accepting `manchester` and `functional` case-insensitively.
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "manchester" | "omn" => Some(Self::Manchester),
            "functional" | "ofn" => Some(Self::Functional),
            _ => None,
        }
    }

    /// Value used in query parameters.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manchester => "manchester",
            Self::Functional => "functional",
        }
    }

    /// Human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Manchester => "Manchester",
            Self::Functional => "functional",
        }
    }

    /// The other syntax, for a toggle link.
    pub fn other(self) -> Self {
        match self {
            Self::Manchester => Self::Functional,
            Self::Functional => Self::Manchester,
        }
    }
}

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
    /// OWL axioms about this resource, grouped by kind.
    pub ontology: Vec<OntologySection>,
    /// Syntax currently used for the ontology section.
    pub ontology_syntax: String,
    /// Query parameter value of the other syntax.
    pub ontology_other_syntax: String,
    /// Human-readable name of the other syntax.
    pub ontology_other_label: String,
    /// HTML representations of subjects and properties of triples where this resource is an object.
    pub inverses: Vec<(String, Vec<String>)>,
    pub duration: String,
    pub github_issue_url: Option<String>,
    pub depiction: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::OwlSyntax;

    #[test]
    fn parses_syntax_names() {
        assert_eq!(OwlSyntax::from_str("manchester"), Some(OwlSyntax::Manchester));
        assert_eq!(OwlSyntax::from_str("OMN"), Some(OwlSyntax::Manchester));
        assert_eq!(OwlSyntax::from_str("Functional"), Some(OwlSyntax::Functional));
        assert_eq!(OwlSyntax::from_str("ofn"), Some(OwlSyntax::Functional));
        assert_eq!(OwlSyntax::from_str("unknown"), None);
    }

    #[test]
    fn toggles_syntax() {
        assert_eq!(OwlSyntax::Manchester.other(), OwlSyntax::Functional);
        assert_eq!(OwlSyntax::Functional.other(), OwlSyntax::Manchester);
        assert_eq!(OwlSyntax::Manchester.as_str(), "manchester");
        assert_eq!(OwlSyntax::Functional.label(), "functional");
    }
}
