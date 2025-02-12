# syntax=docker/dockerfile:1
FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
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

FROM chainguard/wolfi-base AS runtime
COPY --link --from=builder /app/target/release/rickview /usr/local/bin/rickview
WORKDIR /app
CMD ["/usr/local/bin/rickview"]
