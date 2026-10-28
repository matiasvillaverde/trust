# syntax=docker/dockerfile:1.28.0@sha256:bb22d9815c728170f72750f4e5b0d672e06176142e1d602c7e66c050100b7e5b
FROM golang:1.27.3-alpine@sha256:d200d2dd76b03a99b52651fd1325aff7dcd35422c35c01bac56571b0507f3deb AS gosu
ADD --checksum=sha256:cd9719b775dbfedae53923c9b0dc792b66d42c51e0b36652ed6f747fbadc0164 \
    https://codeload.github.com/tianon/gosu/tar.gz/refs/tags/1.19 /tmp/gosu.tar.gz
WORKDIR /src
RUN tar -xzf /tmp/gosu.tar.gz --strip-components=1 \
    && go mod download && go mod verify \
    && CGO_ENABLED=0 go build -mod=readonly -trimpath -buildvcs=false -ldflags='-s -w' -o /out/gosu . \
    && /out/gosu --version

FROM postgres:18.6-alpine@sha256:10a5c69a83035ab5239f5620d708520e8b28f5aa1f2b20143e6024532edadfb5
# Retain PostgreSQL's supported entrypoint/layout; update the OS dependencies
# and rebuild its existing privilege-drop helper with a patched Go runtime.
RUN apk --no-cache upgrade
COPY --from=gosu --chmod=0755 /out/gosu /usr/local/bin/gosu
COPY --from=gosu /src/LICENSE /usr/share/licenses/gosu/LICENSE
RUN gosu --version && gosu postgres id
