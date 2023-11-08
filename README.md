# RickView

[![Latest Version](https://img.shields.io/crates/v/rickview.svg)](https://crates.io/crates/rickview)
[![Lint and Build](https://github.com/konradhoeffner/rickview/actions/workflows/lint_and_build.yml/badge.svg)](https://github.com/konradhoeffner/rickview/actions/workflows/lint_and_build.yml)
![Unsafe forbidden](https://img.shields.io/badge/unsafe-forbidden-success.svg "Unsafe forbidden")
[![RickView @ Leipzig Semantic Web Day 2023 Video](https://img.shields.io/badge/video-8A2BE2)](https://www.youtube.com/watch?v=n8Kb2P8ilwg&t=8250s)
[![DOI](https://zenodo.org/badge/472769298.svg)](https://zenodo.org/badge/latestdoi/472769298)

A quick RDF viewer (browser).
Layout copied from LodView.
See also the unpublished [paper draft](https://github.com/KonradHoeffner/rickview/releases/download/0.0.2/paper.pdf).

## Docker

Try it out with the example knowledge base:

    docker run -p 8080:8080 ghcr.io/konradhoeffner/rickview

### Docker Compose Example

    services:
      rickview:
        image: ghcr.io/konradhoeffner/rickview
        environment:
          - RICKVIEW_KB_FILE=https://raw.githubusercontent.com/hitontology/ontology/dist/all.ttl
          - RICKVIEW_NAMESPACE=http://hitontology.eu/ontology/
          - RICKVIEW_BASE=/ontology
          - RICKVIEW_TITLE=HITO
          - RICKVIEW_SUBTITLE=Health IT Ontology
          - RICKVIEW_EXAMPLES=Study SoftwareProduct ApplicationSystemTypeCatalogue
          - RICKVIEW_HOMEPAGE=https://hitontology.eu
          - RICKVIEW_ENDPOINT=https://hitontology.eu/sparql
          - RICKVIEW_GITHUB=https://github.com/hitontology/ontology
          - RICKVIEW_DOC=https://hitontology.github.io/ontology/
        ports:
          - "127.0.0.1:8080:8080"
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
You can add custom HTML to the index page by adding a `data/body.html` file.
You can add embedded CSS using the `css` environment variable.
By default, the *Roboto* font is used which RickView hosts locally for robustness, speed and to prevent conflicts with European privacy laws.
If this is not an issue for you and, for example, you want to display Chinese or Japanese characters, you could import a Google Font:

    css = "@import url('https://fonts.googleapis.com/css2?family=Noto+Sans+SC:wght@300&display=swap'); body {font-family: 'Noto Sans SC', sans-serif}"

Compile and run with `cargo run` and then open <http://localhost:8080> in your browser.

## Supported File Formats
The recognized formats and extensions are Turtle (`.ttl`), N-Triples (`.nt`), HDT (`.hdt`) as created by [hdt-cpp](https://github.com/rdfhdt/hdt-cpp) and zstd compressed HDT (`.hdt.zst`).

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
All values are rounded and were measured on an old RickView version on an Intel i9-12900k (16 cores, 24 threads) with 32 GB of DDR5-5200 RAM and a Samsung SSD 980 Pro 1 TB on Arch Linux, standard kernel 5.18.
The qbench2 test URI is <http://www.linkedspending.aksw.org/instance/618ac3ec98384f44a9ef142356ce476d>.
Stats for HDT, which uses much less RAM, are not measured yet. 

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
I am very interested in hearing from you using it for your knowledge bases and am happy to assist you setting it up.
Feature and pull requests are welcome, however the goal of RickView is to stay minimalistic and not serve every use case.
Please also consider filling out the [survey](https://forms.gle/wqxdVdn4pDLXgyYZA) so I can see which features are most requested.

### Why no .env support?
I think this would be overkill, as there is already a default configuration file, a custom configuration file, environment variables and Docker Compose supports `.env` out of the box as well.
So my assumption is that you use the configuration file for local development and `.env` with Docker Compose.
However if you need `.env` support outside of Docker Compose, just create an issue with a motivation and I may implement it.

### How can I use it with large knowledge bases?

1. Convert your data to the default HDT format using [hdt-cpp](https://github.com/rdfhdt/hdt-cpp).
2. Deactivate the title and type indexes by setting `large = true` in `data/config.toml` or setting the environment variable `RICKVIEW_LARGE=true`.

Without the indexes, RickView's memory usage is only a few MB above the underlying [HDT Sophia adapter](https://github.com/konradhoeffner/hdt) in-memory graph,
[see benchmarks](https://github.com/KonradHoeffner/sophia_benchmark/blob/master/benchmark_results.ipynb).
For example, RickView on <http://linkedspending.aksw.org/> uses ~ 2.6 GB RAM and contains LinkedSpending 2015, which is 30 GB as uncompressed N-Triples and 413 MB as zstd compressed HDT.

### When to use compression and why not support other compression formats?

[HDT](https://www.rdfhdt.org/) is a compressed binary format that still supports fast querying.
It can be further compressed but then RickView needs to uncompress it before loading, which in a test with a large knowledge base increased loading time from ~15s to ~17s.
Because decompression is done in streaming mode, this restricts the available compressors and may even result in faster loading if you use a slow drive such as an HDD and a fast CPU.
zstd was chosen because it compresses and decompresses quickly with a high ratio, supports streaming, and adds little overhead to the RickView binary.
Brotli compresses extremely slowly on high compression settings while GZip results in much larger file sizes.
If you need support for another streaming compressor, please [create an issue](https://github.com/konradhoeffner/rickview/issues).

### Why does it look exactly like LodView?
1. LodView looks beautiful and works well, the only problems are performance and to a lesser degree simple containerized deployment.
2. LodView is licensed under MIT, so that is allowed. LodView is Copyright (c) 2014 Diego Valerio Camarda and Alessandro Antonuccio.
3. I can focus my limited time on programming instead of design decisions. Other designs may follow later.
4. Users of LodView can switch without training.
5. Performance comparisons are easier when the interface is very similar. 

## Community Guidelines

### Issues and Support
If you have a problem with the software, want to report a bug or have a feature request, please use the [issue tracker](https://github.com/KonradHoeffner/rickview/issues).
If have a different type of request, feel free to send an email to [Konrad](mailto:konrad.hoeffner@uni-leipzig.de).

### Citation

[![DOI](https://zenodo.org/badge/472769298.svg)](https://zenodo.org/badge/latestdoi/472769298)

There is no publication about RickView yet, so please cite our Zenodo archive for now.

#### BibTeX entry

    @software{rickview,
      author       = {Konrad H{\'o}ffner},
      title        = {{R}ick{V}iew: {L}ightweight Standalone Knowledge Graph Browsing Powered by {R}ust},
      year         = 2023,
      publisher    = {Zenodo},
      version      = {x.y.z},
      doi          = {10.5281/zenodo.8290117},
      url          = {https://doi.org/10.5281/zenodo.8290117}
    }

#### Citation string

Konrad Höffner (2023). RickView: Lightweight Standalone Knowledge Graph Browsing Powered by Rust. https://doi.org/10.5281/zenodo.8290117

### Contribute
We are happy to receive pull requests.
Please use `cargo fmt` before committing and make sure that that the code compiles on the newest nightly toolchain with the default features.
Browse the default knowledge base after `cargo run` and verify that nothing is broken.
`cargo clippy` should not report any warnings.
You can also contribute by recommending RickView and by [sharing your RickView deployments](https://github.com/KonradHoeffner/hdt/issues/35>).
