# syntax=docker/dockerfile:1
# Build-stage digests verified 2026-10-26; release currently targets linux/amd64.
FROM node:24.13.0-bookworm-slim@sha256:4660b1ca8b28d6d1906fd644abe34b2ed81d15434d26d845ef0aced307cf4b6f AS assets
WORKDIR /build
COPY package.json package-lock.json ./
RUN npm ci --ignore-scripts
COPY app/assets app/assets
COPY app/templates app/templates
COPY app/src app/src
RUN npm run assets && cp node_modules/htmx.org/dist/htmx.min.js app/assets/htmx.min.js

FROM rust:1.99.0-slim-bookworm@sha256:2c3a22f0a5533ea2dd5a16627bc841228151faa2d4de2644ac9987e4a2f1f2fa AS build
RUN apt-get update && apt-get install --no-install-recommends -y pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY app/Cargo.toml app/build.rs app/openapi.json app/
COPY app/src app/src
COPY app/templates app/templates
COPY app/migrations app/migrations
COPY app/tests/fixtures app/tests/fixtures
COPY --from=assets /build/app/assets app/assets
COPY deploy/healthcheck.rs /build/healthcheck.rs
RUN rustc --edition=2024 -C opt-level=s -C strip=symbols /build/healthcheck.rs -o /build/todoer-healthcheck
ARG TODOER_BUILD_VERSION=0.1.0
RUN TODOER_BUILD_VERSION="$TODOER_BUILD_VERSION" cargo build --locked --release --workspace

# Runtime digest verified 2026-10-29, including amd64 OpenSSL/glibc/libgcc libraries.
# Contains only runtime libraries and CA roots: no shell, curl or package manager.
# Keep the explicit distro and digest; refresh through reviewed dependency updates.
FROM gcr.io/distroless/cc-debian13:nonroot@sha256:3ee00291cf386ec52a552e6a858d555debd74776eb4bfa53fa3df2cd6677515e AS runtime
WORKDIR /app
COPY --from=build /build/target/release/todoer-server /build/target/release/todoer /build/target/release/todoer-migrate /usr/local/bin/
COPY --from=build /build/todoer-healthcheck /usr/local/bin/todoer-healthcheck
COPY --from=assets /build/app/assets /app/app/assets
ENV TODOER_RUST_DB=/data/todoer.db TODOER_RUST_BIND=0.0.0.0 TODOER_RUST_PORT=8092
USER 10001:10001
EXPOSE 8092
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 CMD ["/usr/local/bin/todoer-healthcheck"]
ENTRYPOINT ["todoer-server"]
