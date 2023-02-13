#[cfg(feature = "hdt")]
use crate::rdf::GraphEnum;
use crate::rdf::{graph, titles, types};
use bytesize::ByteSize;
use deepsize::DeepSizeOf;
const VERSION: &str = env!("CARGO_PKG_VERSION");

use serde::Serialize;
#[derive(Serialize, Debug)]
pub struct About {
    pub cargo_pkg_version: &'static str,
    pub num_titles: usize,
    pub num_types: usize,
    pub titles_size: String,
    pub types_size: String,
    pub graph_size: Option<String>,
}

impl About {
    pub fn new() -> About {
        let graph_size = match graph() {
            #[cfg(feature = "hdt")]
            GraphEnum::HdtGraph(hdt_graph) => Some(ByteSize(hdt_graph.size_in_bytes() as u64).to_string()),
            //GraphEnum::FastGraph(fast_graph) => Some(fast_graph.get_wrapped().get_wrapped().len().to_string() + " triples"),
            GraphEnum::FastGraph(_) => Some("unknown number of triples".to_owned()),
        };
        About {
            cargo_pkg_version: VERSION,
            num_titles: titles().len(),
            num_types: types().len(),
            types_size: ByteSize(types().deep_size_of() as u64).to_string(),
            titles_size: ByteSize(titles().deep_size_of() as u64).to_string(),
            graph_size,
        }
    }
}
