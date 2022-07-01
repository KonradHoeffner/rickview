---
title: 'RickView---A performant Linked Data browser'
#title: 'RickView---A quick RDF viewer written in Rust'
tags:
  - linked data
  - linked data browser
  - semantic web 
  - rdf
  - visualization
authors:
  - name: Konrad Höffner
    orcid: 0000-0001-7358-3217
    corresponding: true # (This is how to denote the corresponding author)
    affiliation: 1
affiliations:
 - name: Institute for Medical Informatics, Statistics and Epidemiology, Medical Faculty, Leipzig University, Germany
   index: 1
date: 20 June 2022
bibliography: paper.bib
---

# Summary
Search, browsing and exploration is a critical area in the linked data (LD) lifecycle.
Similar to web browsers, where users enter the URL of a website and receive a display of an HTML document, in linked data browsers, users enter the URL of an RDF resource and receive a human readable representation.
However in contrast to websites, RDF resources are intended to be processed by machines and do not provide a standardized way of being displayed.

An RDF graph, also called a knowledge base, is a set of RDF triples in the form of (subject, predicate, object) and typically describes a common domain under a shared namespace, which can be abbreviated with a prefix, such as `rdf`.
For example, the RDF vocabulary has a "hash namespace" (ending with a `#` character) of `http://www.w3.org/1999/02/22-rdf-syntax-ns#`, which signifies that all resources, like `rdf:type` and `rdf:property`, of the vocabulary are described in the same document, which is suitable for small graphs such as ontologies that are not intended to be viewed by non-expert users.

RDF browsers on the other hand *resolve URIs* (URLs) of knowledge bases with "slash namespaces" (ending with a '/' character), so that each resource has it's associated page.
Ideally, they offer a machine processable RDF serialization as well, based on either content negotation, POST parameters or URL variants such as `http://mynamespace/myresource.ttl`.
LD browsers describe the direct neighbourhood of a resource in an RDF graph, that is they list all triples where the given resource is either the subject (a direct connection) or the object (an inverse connection).

We present RickView, an RDF browser with the following design goals:

* performance: low memory and CPU utilization, fast page load times and high number of handled requests
* robustness: once it compiles, it runs indefinitely
* standalone: no dependance on services such as a SPARQL endpoint or a web server
* adaptability to any small to medium sized knowledge base via sensible defaults that can be changed in a configuration file and overridden with environment variables
* containerization: offer a compact container image containing a statically linked binary
* simplicity: RickView is minimalistic and only offers browsing of static data

Non goals: RickView is a read-only visualization for static data, does not offer other visualization types [@linkeddatavisualization], such as graph based, and is not a search engine.
It is not designed for data that does not fit on one machine ("big data") or indeed data that does not fit on RAM.
Large knowledge bases, such as the complete DBpedia, fit better with the traditional SPARQL endpoint paradigm.

# Statement of need

While initial enthusiasm in the Semantic Web field has led to a large amount of published knowledge bases, mainstream adoption has stagnated due to a lack of freely available performant, accessible, robust and adaptable tools [@semanticwebreview]. 
Instead, limited duration research grants motivate the proliferation of countless research prototypes, which are not optimized for any of those criteria, are not maintained after the project ends and compete for resources on crowded servers if they do not break down completely.
While there are are several existing RDF browsers, they are not optimized for performance.

# Implementation 

The standard backend for LD projects are *SPARQL endpoints*, which allow expressive SQL-like *SPARQL queries*, however they are overengineered for the simple task of browsing.
For example, the popular Virtuoso Open-Source Edition maps SPARQL to SQL queries on top of a relational database, which is currently faster than native triplestores like Apache Jena or but requires tuning of parameters like memory buffer sizes for optimal resource allocation and the RDF data model can cause large amounts of joins, which negatively impacts query runtime.

RickView instead follows an alternative approach of directly querying an in-memory dataset bypassing SPARQL.
RickView is an LD browser written in Rust, which enables a high level of performance comparable to C and C++ while being memory-safe and thread-safe by design. 
In order to keep the focus on performance and to get a baseline of functionality for performance evaluation, the design is in large parts copied over from LodView [@lodview].

It is already used in the SNIK [@snik] and HITO [@hito] research projects as a LodView replacement and in LinkedSpending [@linkedspending] as a replacement for Virtuoso SPARQL Endpoint + OntoWiki.

# References
