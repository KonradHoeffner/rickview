use serde::{Serialize};

#[derive(Serialize)]
pub struct Resource {
    uri: String,
    suffix: String,
    time: String
}
