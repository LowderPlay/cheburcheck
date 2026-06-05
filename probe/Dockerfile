# syntax=docker/dockerfile:1.10

FROM docker.io/rust:1-slim-bookworm AS build

WORKDIR /build

COPY . .

RUN --mount=type=cache,id=probe-target,target=/build/target \
    --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    set -eux; \
    RUSTFLAGS="-C strip=symbols" cargo build --locked --release --package probe --bin cheburprobe; \
    cp target/release/cheburprobe ./probe-bin

FROM docker.io/debian:bookworm-slim

RUN groupadd --system app && \
    useradd --system --gid app --home-dir /app --shell /usr/sbin/nologin app

WORKDIR /app

COPY --from=build /build/probe-bin ./probe

USER app

ENTRYPOINT ["./probe"]
