FROM rust:1.90-bookworm AS builder
WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY xtask ./xtask

RUN cargo build --release -p qm-server

FROM node:25-bookworm AS web-builder
WORKDIR /src

ENV VOLTA_HOME=/root/.volta
ENV VOLTA_FEATURE_PNPM=1
ENV PATH=$VOLTA_HOME/bin:$PATH

RUN curl https://get.volta.sh | bash \
    && volta install node@25.9.0 pnpm@10.33.2

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY openapi.json ./openapi.json
COPY web ./web

RUN pnpm install --frozen-lockfile
RUN pnpm -C web generate:api
RUN pnpm -C web build

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app quartermaster \
    && mkdir -p /data \
    && chown -R quartermaster:quartermaster /app /data

WORKDIR /app
COPY --from=builder /src/target/release/qm-server /usr/local/bin/qm-server
COPY --from=web-builder /src/web/build /app/web

ENV QM_WEB_DIST_DIR=/app/web

USER quartermaster
EXPOSE 8080
VOLUME ["/data"]

CMD ["qm-server"]
