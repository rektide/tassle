# syntax=docker/dockerfile:1

# =============================================================================
# Stage: builder
# Builds all workspace binaries in release mode.
# =============================================================================
FROM rust:1.89-bookworm AS builder

WORKDIR /src

# Cache dependencies by building a dummy project first
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask

# Build the workspace
RUN cargo build --release --workspace

# =============================================================================
# Stage: tass
# Minimal runtime image for the `tass` CLI.
# =============================================================================
FROM debian:testing-slim AS tass

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tass /usr/local/bin/tass

ENTRYPOINT ["tass"]

# =============================================================================
# Stage: tass-codegen
# Minimal runtime image for the `tass-codegen` binary.
# =============================================================================
FROM debian:testing-slim AS tass-codegen

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tass-codegen /usr/local/bin/tass-codegen

ENTRYPOINT ["tass-codegen"]

# =============================================================================
# Stage: tass-listen
# Minimal runtime image for the `tass-listen` daemon.
# =============================================================================
FROM debian:testing-slim AS tass-listen

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tass-listen /usr/local/bin/tass-listen

EXPOSE 3000

ENTRYPOINT ["tass-listen"]

# =============================================================================
# Stage: tass-validate
# Minimal runtime image for the `tass-validate` binary.
# =============================================================================
FROM debian:testing-slim AS tass-validate

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tass-validate /usr/local/bin/tass-validate

ENTRYPOINT ["tass-validate"]

# =============================================================================
# Stage: tass-web
# Minimal runtime image for the `tass-web` axum server.
# =============================================================================
FROM debian:testing-slim AS tass-web

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/target/release/tass-web /usr/local/bin/tass-web

EXPOSE 3000

ENTRYPOINT ["tass-web"]
