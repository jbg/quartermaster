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

ARG BUILD_VERSION=0.0.0-dev
ARG HASS_ARCH=amd64
ARG VCS_REF=unknown
ARG BUILD_DATE=unknown

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gosu jq \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /app quartermaster \
    && mkdir -p /data \
    && chown -R quartermaster:quartermaster /app /data

WORKDIR /app
COPY --from=builder /src/target/release/qm-server /usr/local/bin/qm-server
COPY --from=web-builder /src/web/build /app/web
COPY --chmod=0755 docker/entrypoint.sh /usr/local/bin/quartermaster-entrypoint

ENV QM_WEB_DIST_DIR=/app/web

LABEL io.hass.version="${BUILD_VERSION}" \
      io.hass.type="app" \
      io.hass.arch="${HASS_ARCH}" \
      org.opencontainers.image.title="Quartermaster" \
      org.opencontainers.image.description="Self-hostable kitchen inventory for households" \
      org.opencontainers.image.url="https://github.com/jbg/quartermaster" \
      org.opencontainers.image.source="https://github.com/jbg/quartermaster" \
      org.opencontainers.image.version="${BUILD_VERSION}" \
      org.opencontainers.image.revision="${VCS_REF}" \
      org.opencontainers.image.created="${BUILD_DATE}" \
      org.opencontainers.image.licenses="Apache-2.0"

EXPOSE 8080
VOLUME ["/data"]

ENTRYPOINT ["quartermaster-entrypoint"]
CMD ["qm-server"]
