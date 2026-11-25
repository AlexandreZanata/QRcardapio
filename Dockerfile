# syntax=docker/dockerfile:1

FROM rust:1.100-slim-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY templates ./templates
COPY static ./static
COPY i18n ./i18n
COPY migrations ./migrations
COPY contracts ./contracts

RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    poppler-utils \
    tesseract-ocr \
    tesseract-ocr-por \
    tesseract-ocr-eng \
    tesseract-ocr-spa \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system --gid 10001 qrcardapio \
    && useradd --system --uid 10001 --gid qrcardapio --home-dir /app --shell /usr/sbin/nologin qrcardapio \
    && mkdir -p /data/db /data/media /data/published/.tmp \
    && chown -R qrcardapio:qrcardapio /data

WORKDIR /app

COPY --from=builder /app/target/release/qrcardapio /usr/local/bin/qrcardapio
COPY static ./static

ENV BIND_ADDR=0.0.0.0:8080 \
    DATA_DIR=/data \
    STATIC_DIR=/app/static \
    LOG_FORMAT=json \
    LOG_LEVEL=info,qrcardapio=info \
    COOKIE_SECURE=true \
    TRUST_PROXY=true

USER qrcardapio

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
    CMD ["curl", "-fsS", "http://127.0.0.1:8080/healthz"]

ENTRYPOINT ["/usr/local/bin/qrcardapio"]
CMD ["serve"]
