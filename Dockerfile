# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b
FROM rust:1.99.1-trixie@sha256:931c2c0531a8b143e044f0f3914dfe802f15bb3d1955f0b657e9359327ac2ffc AS build
RUN apt-get update && apt-get install -y --no-install-recommends libdbus-1-dev libssl-dev libsqlite3-dev pkg-config && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --locked --release -p trust-service -p trust-cli --bin trust-service --bin trust \
    && install -Dm755 target/release/trust-service /out/trust-service \
    && install -Dm755 target/release/trust /out/trust

FROM cgr.dev/chainguard/wolfi-base:latest@sha256:360896efb0753a5701c48f860e17130c205eae5a264797aa3e7ea126ee2923a7 AS runtime
RUN apk --no-cache upgrade \
    && apk add --no-cache ca-certificates-bundle libgcc libssl3 sqlite-libs dbus-libs curl \
    && addgroup -g 10001 trust \
    && adduser -D -H -u 10001 -G trust -s /sbin/nologin trust \
    && mkdir -p /data /etc/trust \
    && chown 10001:10001 /data /etc/trust \
    && chmod 0700 /data /etc/trust
COPY --from=build /out/trust-service /out/trust /usr/local/bin/
USER 10001:10001
WORKDIR /data
EXPOSE 8000
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=3s --start-period=15s --retries=3 \
    CMD curl --fail --silent --max-time 2 http://127.0.0.1:8000/readyz >/dev/null || exit 1
ENTRYPOINT ["/usr/local/bin/trust-service"]
CMD ["/etc/trust/config.json"]
