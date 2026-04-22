FROM rust:1.90-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask

RUN cargo build --release -p qm-server

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app quartermaster \
    && mkdir -p /data \
    && chown -R quartermaster:quartermaster /app /data

WORKDIR /app
COPY --from=builder /src/target/release/qm-server /usr/local/bin/qm-server

USER quartermaster
EXPOSE 8080
VOLUME ["/data"]

CMD ["qm-server"]
