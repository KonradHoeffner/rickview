# RickView

![Unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg "Unsafe forbidden")

A quick RDF viewer (browser).
Layout copied from LodView.
See also the unpublished [paper draft](https://github.com/KonradHoeffner/rickview/releases/download/0.0.2/paper.pdf).

## Docker

Try it out with the example knowledge base:

    docker run -p 8080:8080 ghcr.io/konradhoeffner/rickview

### Docker Compose Example

    services:
      ontology: # if you need a separate build step, otherwise remove this service and use a bind mount
        build: ./ontology # merge all Turtle files into one
        volumes:
          - rdf:/ontology/dist
      rickview:
        build: ./rickview
        environment:
          - RICKVIEW_KB_FILE=/rdf/hito.ttl
          - RICKVIEW_NAMESPACE=http://hitontology/ontology/
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

## Precompiled Binaries
Download the binary from the [latest release](https://github.com/konradhoeffner/rickview/releases/latest) and run `rickview`.
If you need binaries for a different platform than Linux amd64, [let me know](https://github.com/konradhoeffner/rickview/issues/new).

## Compile it yourself
Alternatively, you can compile it for your own platform with `cargo install rickview`.
Or you can clone [the repository](https://github.com/konradhoeffner/rickview) and then `cargo build`.
This requires you to install [Rust including Cargo](https://www.rust-lang.org/tools/install).
I recommend installing Rust via `rustup`, especially on Ubuntu, because `apt install cargo` may install an old version and not work correctly with RickView.
On Arch Linux however, both `pacman -S rustup` and `pacman -S rust` worked in my testing as the Rust package was much more current and does not require a separate step of choosing between stable and unstable rust.

## Configure
Default configuration is stored in `data/default.toml`, which you can override with a custom `data/config.toml` or environment variables.
Configuration keys are in lower\_snake\_case, while environment variables are prefixed with RICKVIEW\_ and are in SCREAMING\_SNAKE\_CASE.
For example, `namespace = "http://hitontology.eu/ontology/"` in `config.toml` is equivalent to `RICKVIEW_NAMESPACE=http://hitontology.eu/ontology/` as an environment variable.
You need to provide a knowledge base in RDF Turtle or HDT format, whose default path is `data/kb.ttl`.
If you don't, RickView will show a minimal example knowledge base.
Compile and run with `cargo run` and then open <http://localhost:8080> in your browser.

## Logging
The default log level is "info" for RickView and "error" for libraries.
Change the log level of RickView with the `log_level` configuration key or the `RICKVIEW_LOG_LEVEL` environment variable.
Override this setting using the `RUST_LOG` env var to configure the log levels of dependencies, see the [env\_logger documentation](https://docs.rs/env_logger/latest/env_logger/), for example:

    RUST_LOG=rickview=debug cargo run

## Motivation
Existing RDF browsers like [LodView](https://github.com/LodLive/LodView/) look great but use too much hardware ressources as they are based on interpreted or garbage collected languages.
This leads to long wait times and out of memory errors on typical small scale research VMs with dozens of docker containers for longtime archival of finished research projects, whose results should still be available to enable reproducible science.

## Goals
Implement a basic RDF browser similar to LodView in Rust with the following goals:

* speed
* low resource utilization
* good design
* option to generate static HTML

## Stats
All values are rounded and are measured on an Intel i9-12900k (16 cores, 24 threads) with 32 GB of DDR5-5200 RAM and a Samsung SSD 980 Pro 1 TB on Arch Linux, standard kernel 5.18.
The qbench2 test URI is <http://www.linkedspending.aksw.org/instance/618ac3ec98384f44a9ef142356ce476d>.

* Linux x86-64 release binary size (strip, fat link time optimization, all features): 4.1 MB
* Linux x86-64 release binary size (strip, no link time optimization, all features): 5.8 MB
* Docker image size: 9.7 MB
* release compile time (strip, fat LTO, all features, cold): 52 s
* release compile time (strip, no LTO, all features, cold): 19 s
* RAM consumption (FastGraph, docker stats, HITO knowledge base 1.1 MB, idle): 10 MB
* RAM consumption (FastGraph, docker stats, HITO knowledge base 1.1 MB, 30s max load): 15 MB
* RAM consumption (LightGraph, docker stats, qbench2, 16.2 million triples, 3.6 GB N-Triples file): 2.568 GiB
* RAM consumption (LightGraph, qbench2, normalized): 2.524 GiB
* RAM consumption (FastGraph, docker stats, qbench2):  4.9 GB
* graph loading time (FastGraph, qbench2, debug build): 548-552 s
* graph loading time (FastGraph, qbench2): 47 s
* graph loading time (LightGraph, qbench2): 40 s
* HTML page generation time (LightGraph, qbench2): 1.9 s
* HTML page generation time (FastGraph, qbench2): 6-58 ms
* Turtle page generation time (FastGraph, qbench2): 6-35 ms

### Throughput Single Resource, HTML
There is no page cache but there could still be internal caching benefits so this should be more elaborate in the future.

    $ wrk -t 24 -c 24 -d 30 http://localhost:8080/SoftwareProduct -H "Accept: text/html"
    Running 30s test @ http://localhost:8080/SoftwareProduct
      24 threads and 24 connections
      Thread Stats   Avg      Stdev     Max   +/- Stdev
        Latency     9.79ms    3.25ms  26.92ms   56.40%
        Req/Sec   102.36     36.17   212.00     66.74%
      73590 requests in 30.02s, 1.04GB read
    Requests/sec:   2451.31
    Transfer/sec:     35.43MB

### Throughput Single Resource, RDF Turtle

    $ docker run --network=host -v $PWD/ontology/dist/hito.ttl:/app/data/kb.ttl  rickview
    $ wrk -t 24 -c 24 -d 30 http://localhost:8080/SoftwareProduct
    Running 30s test @ http://localhost:8080/SoftwareProduct
      24 threads and 24 connections
      Thread Stats   Avg      Stdev     Max   +/- Stdev
        Latency    13.96ms    4.74ms  37.20ms   55.04%
        Req/Sec    71.77     26.17   121.00     66.43%
      51572 requests in 30.02s, 567.72MB read
    Requests/sec:   1717.72
    Transfer/sec:     18.91MB

## Stats of LodView
For comparison, here are the stats for the LodView RDF browser, written in Java and Swing.

* war file:  21 MB (twice the size of RickView)
* Docker image size: 246 MB (25 times the size of RickView)
* compile and package time: 12 s (compiles 4 times faster than RickView with all optimizations)
* RAM consumption (docker stats, HITO knowledge base 1.1 MB, idle, excluding Virtuoso): 482 MB (48 times more RAM usage)
* RAM consumption (docker stats, HITO knowledge base 1.1 MB, 30s max load, excluding Virtuoso): 790 MB (53x)
* RAM consumption (docker stats, HITO knowledge base 1.1 MB, idle, including Virtuoso): 655 MB (65x)

### Throughput Single Resource
As data is loaded after page load via JavaScript, real world performance may worse.

    $ wrk -t 24 -c 24 -d 30 http://localhost:8104/ontology/SoftwareProduct
    Running 30s test @ http://localhost:8104/ontology/SoftwareProduct
      24 threads and 24 connections
      Thread Stats   Avg      Stdev     Max   +/- Stdev
        Latency     1.97ms    2.40ms  44.84ms   88.08%
        Req/Sec   713.62    353.46     1.24k    38.76%
    511567 requests in 30.03s, 1.61GB read
      Socket errors: connect 0, read 1, write 0, timeout 0
      Non-2xx or 3xx responses: 511567
    Requests/sec:  17037.90
    Transfer/sec:     55.07MB

LodView was not able to serve 24 threads and 24 connections, so try it with only 1 thread and 1 connection:

    $ wrk -t 1 -c 1 -d 30 http://localhost:8104/ontology/SoftwareProduct
    Running 30s test @ http://localhost:8104/ontology/SoftwareProduct
      1 threads and 1 connections
      Thread Stats   Avg      Stdev     Max   +/- Stdev
        Latency     2.90ms   13.30ms 250.66ms   97.34%
        Req/Sec   715.41    251.08     1.48k    69.46%
      21227 requests in 30.01s, 68.61MB read
      Non-2xx or 3xx responses: 21227
    Requests/sec:    707.24
    Transfer/sec:      2.29MB

Even a single thread and a single connection cause the container to report errors, this will be investigated in the future.

## FAQ

### Why is RickView necessary? Performance doesn't matter and RAM costs almost nothing!
According to [Hitzler 2021](https://cacm.acm.org/magazines/2021/2/250085-a-review-of-the-semantic-web-field/fulltext?mobile=false), mainstream adoption of the Semantic Web field has stagnated due to a lack of freely available performant, accessible, robust and adaptable tools.
Instead, limited duration research grants motivate the proliferation of countless research prototypes, which are not optimized for any of those criteria, are not maintained after the project ends and finally compete for resources on crowded servers if they do not break down completely.

### Can you implement feature X?
As RickView is still in the prototype stage, I would be very interested in hearing from you using it for your knowledge bases and am happy to assist you setting it up.
Feature and pull requests are welcome, however the goal of RickView is to stay minimalistic and not serve every use case.

### Why no .env support?
I think this would be overkill, as there is already a default configuration file, a custom configuration file, environment variables and Docker Compose supports `.env` out of the box as well.
So my assumption is that you use the configuration file for local development and `.env` with Docker Compose.
However if you need `.env` support outside of Docker Compose, just create an issue with a motivation and I may implement it.

### Can I use it with DBpedia?
RickView is not designed for large knowledge bases (many GB) such as the complete DBpedia, as it holds the knowledge base in RAM.
In those cases, traditional RDF browsers based on a SPARQL endpoint are the better solution.
However I do plan to test and if possible support knowledge bases up to around 2 GB of compressed size.

### Why does it look exactly like LodView?
1. LodView looks beautiful and works well, the only problems are performance and to a lesser degree simple containerized deployment.
2. LodView is licensed under MIT, so that is allowed. LodView is Copyright (c) 2014 Diego Valerio Camarda and Alessandro Antonuccio.
3. I can focus my limited time on programming instead of design decisions. Other designs may follow later.
4. Users of LodView can switch without training.
5. Performance comparisons are easier when the interface is very similar. 
