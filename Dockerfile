# syntax=docker/dockerfile:1.4
FROM clux/muslrust:1.61.0 AS chef
USER root
RUN cargo install cargo-chef
WORKDIR /app

FROM chef AS planner
COPY --link . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --link --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --target x86_64-unknown-linux-musl --recipe-path recipe.json
COPY --link . .
RUN cargo build --release --target x86_64-unknown-linux-musl

FROM alpine AS runtime
RUN addgroup -S myuser && adduser -S myuser -G myuser
COPY --link --from=builder /app/target/x86_64-unknown-linux-musl/release/rickview /usr/local/bin/
USER myuser
WORKDIR /app
RUN mkdir -p data && touch data/kb.ttl
CMD ["/usr/local/bin/rickview"]
