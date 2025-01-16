# syntax=docker/dockerfile:1
FROM clux/muslrust:1.84.0-stable AS chef
USER root
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
RUN cargo build --release

FROM busybox AS runtime
RUN addgroup -S myuser && adduser -S myuser -G myuser
COPY --link --from=builder /app/target/*/release/rickview /usr/local/bin/
USER myuser
WORKDIR /app
RUN mkdir -p data && touch data/kb.ttl
CMD ["/usr/local/bin/rickview"]
