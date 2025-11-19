FROM docker.io/rust:1-slim-bookworm AS build

## cargo package name: customize here or provide via --build-arg
ARG pkg=blocklist-check

WORKDIR /build

COPY . .

RUN apt update && apt install -y libssl-dev pkg-config

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    cargo build --release; \
    objcopy --compress-debug-sections target/release/$pkg ./main

################################################################################

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN apt update && apt install -y libssl3 ca-certificates

## copy the main binary
COPY --from=build /build/main ./

COPY --from=build /build/static ./static
COPY --from=build /build/templates ./templates

## ensure the container listens globally on port 8080
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8080

EXPOSE 8080

CMD ./main
