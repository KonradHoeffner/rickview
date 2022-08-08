# RickView

![Unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg "Unsafe forbidden")

A quick RDF viewer (browser).
Prototype written in Rust.
Layout partially copied from LodView.

## Install
Install [Rust including Cargo](https://www.rust-lang.org/tools/install) and clone this repository.
Binaries and prebuild Docker images will be available in the future.

## Configure
Default configuration is stored in `data/default.toml`, which you can override with a custom `data/config.toml` or environment variables.
Configuration keys are in lower\_snake\_case, while environment variables are prefixed with RICKVIEW\_ and are in SCREAMING\_SNAKE\_CASE.
For example, `namespace = "http://hitontology.eu/ontology/"` in `config.toml` is equivalent to `RICKVIEW_NAMESPACE=http://hitontology.eu/ontology/` as an environment variable.
You need to provide a knowledgebase in RDF Turtle format, whose default path is `data/kb.ttl`.
Compile and run with `cargo run` and then open <http://localhost:8080>` in your browser.

## Run

    cargo run

## Build

    cargo build --release

## Docker

    docker build . -t rickview
    docker run --mount "type=bind,src=$PWD/data/kb.ttl,target=/app/data/kb.ttl"  --network="host" rickview

## Docker Compose Example

    services:
      ontology:
        build: ./ontology
        volumes:
          - rdf:/ontology/dist
      rickview:
        build: ./rickview
        environment:
          - RICKVIEW_KB_FILE=/rdf/hito.ttl
          - RICKVIEW_BASE_PATH=/ontology
          - RICKVIEW_TITLE=HITO
          - RICKVIEW_SUBTITLE=Health IT Ontology
          - RICKVIEW_EXAMPLES=Study SoftwareProduct ApplicationSystemTypeCatalogue
          - RICKVIEW_HOMEPAGE=https://hitontology.eu
          - RICKVIEW_ENDPOINT=https://hitontology.eu/sparql
          - RICKVIEW_GITHUB=https://github.com/hitontology/ontology
          - RICKVIEW_DOC=https://hitontology.github.io/ontology/index-en.html
        volumes:
          - rdf:/rdf
        ports:
          - "127.0.0.1:8104:8080"
        restart: unless-stopped

## Motivation
Existing RDF browsers like [LodView](https://github.com/LodLive/LodView/) look great but use too much hardware ressources as they are based on interpreted or garbage collected languages.
This leads to long wait times and out of memory errors on typical small scale research VMs with dozens of docker containers for longtime archival of finished research projects, whose results should still be available to enable reproducible science.

## Goals
Implement a basic RDF browser similar to LodView in Rust with the following goals:

* speed
* low resource utilization
* good design
* option to generate static HTML

## FAQ

### Why no .env support?
I think this would be overkill, as there is already a default configuration file, a custom configuration file, environment variables and Docker Compose supports `.env` out of the box as well.
So my assumption is that you use the configuration file for local development and `.env` with Docker Compose.
However if you need `.env` support outside of Docker Compose, just create an issue with a motivation and I may implement it.
