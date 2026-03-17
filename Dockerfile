# syntax=docker/dockerfile:1
#FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
# can't use prebuild chef anymore as nightly Rust needed for transitive QWT dep
FROM rustlang/rust:nightly AS chef
RUN cargo install --locked cargo-chef 
WORKDIR /app

FROM chef AS planner
ARG CARGO_INCREMENTAL=0
COPY . .
RUN cargo +nightly chef prepare --recipe-path recipe.json

FROM chef AS builder
ARG CARGO_INCREMENTAL=0
COPY --link --from=planner /app/recipe.json recipe.json
RUN cargo +nightly chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo +nightly build --release --bin rickview

FROM chainguard/wolfi-base AS runtime
COPY --link --from=builder /app/target/release/rickview /usr/local/bin/rickview
WORKDIR /app
CMD ["/usr/local/bin/rickview"]
