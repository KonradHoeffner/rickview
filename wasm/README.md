# RickView WASM

A client-side HDT viewer that reuses RickView's rendering core. Everything stays in the browser; the file is never uploaded.

## Build

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack
./build.sh
python3 -m http.server --directory www 8080
```

Then open <http://localhost:8080/> and choose a `.hdt` or `.hdt.zst` file. You can also pass a URL and the namespace as query parameters:

```
http://localhost:8080/?kb=https://example.org/kb.hdt&ns=https://example.org/resource/&prefix=ex
```

The viewer uses hash routing, so resources look like `#/ResourceSuffix`. N-Triples and Turtle can be downloaded from a resource page.

## Scope

* HDT and HDT.zst input only. Turtle, N-Triples, RDF/XML and directory input stay server-only.
* No OWL axioms and no RDF/XML output.
* No content negotiation for external clients: this is a viewer, not a Linked Data server.
* One graph per page; reload to open another file.
* Files above 20 MB are configured with `large = true` to skip the title/type precomputation.
