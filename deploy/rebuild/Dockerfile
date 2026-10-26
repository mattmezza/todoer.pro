# syntax=docker/dockerfile:1
# Registry index digests verified 2026-10-26; release currently targets linux/amd64.
FROM node:24.13.0-bookworm-slim@sha256:4660b1ca8b28d6d1906fd644abe34b2ed81d15434d26d845ef0aced307cf4b6f AS assets
WORKDIR /build
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts
COPY rebuild/assets rebuild/assets
COPY rebuild/templates rebuild/templates
COPY rebuild/src rebuild/src
RUN npm run assets && cp node_modules/htmx.org/dist/htmx.min.js rebuild/assets/htmx.min.js

FROM rust:1.99.0-slim-bookworm@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa AS build
RUN apt-get update && apt-get install --no-install-recommends -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY rebuild/Cargo.toml rebuild/build.rs rebuild/openapi.json rebuild/
COPY rebuild/src rebuild/src
COPY rebuild/templates rebuild/templates
COPY rebuild/migrations rebuild/migrations
COPY rebuild/tests/fixtures rebuild/tests/fixtures
COPY --from=assets /build/rebuild/assets rebuild/assets
RUN cargo test --locked --workspace && cargo build --locked --release --workspace

FROM debian:bookworm-slim@sha256:7ff6ba31527b9e336162fa061746c366c4b2032f4d2cff2078bea43f78efe3f8 AS runtime
RUN apt-get update && apt-get install --no-install-recommends -y ca-certificates curl libgcc-s1 libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 todoer && useradd --uid 10001 --gid 10001 --no-create-home todoer \
    && mkdir -p /app/rebuild/assets /app/public /data && chown 10001:10001 /data
WORKDIR /app
COPY --from=build /build/target/release/todoer-server /build/target/release/todoer /build/target/release/todoer-migrate /usr/local/bin/
COPY --from=assets /build/rebuild/assets /app/rebuild/assets
COPY public/favicon.png public/icon512_rounded.png public/icon512_maskable.png /app/public/
ENV TODOER_RUST_DB=/data/todoer.db TODOER_RUST_BIND=0.0.0.0 TODOER_RUST_PORT=8092
USER 10001:10001
EXPOSE 8092
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 CMD curl --fail --silent --max-time 2 http://127.0.0.1:8092/ready || exit 1
ENTRYPOINT ["todoer-server"]
