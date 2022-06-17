# RickView

A quick RDF viewer (browser).
Very early prototype written in Rust.
Layout partially copied from LodView.

## Problem
Existing RDF browsers like [LodView](https://github.com/LodLive/LodView/) look great but use too much hardware ressources as they are based on interpreted or garbage collected languages.
This leads to long wait times and out of memory errors on typical small scale research VMs with dozens of docker containers for longtime archival of finished research projects, whose results should still be available to enable reproducible science.

## Goals
Implement a basic RDF browser similar to LodView in Rust with the following goals:

* speed
* low resource utilization
* good design
* option to generate static HTML
