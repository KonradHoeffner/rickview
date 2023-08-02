# syntax=docker/dockerfile:1
#=======
FROM rust:1 AS chef
USER root
RUN rustup toolchain install nightly-2024-01-31 && rustup default nightly-2024-01-31
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
ARG CARGO_INCREMENTAL=0
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG CARGO_INCREMENTAL=0
COPY --link --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin rickview

#FROM alpine AS runtime
#RUN apk add libgcc
FROM debian:stable-slim AS runtime
COPY --link --from=builder /app/target/release/rickview /usr/local/bin/rickview
WORKDIR /app
RUN mkdir -p data && touch data/kb.ttl
CMD ["/usr/local/bin/rickview"]
