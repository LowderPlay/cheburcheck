# syntax=docker/dockerfile:1.10

FROM docker.io/rust:1-slim-bookworm AS build

WORKDIR /build

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl-dev pkg-config && \
    rm -rf /var/lib/apt/lists/*

COPY . .

RUN --mount=type=cache,id=website-target,target=/build/target \
    --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    set -eux; \
    RUSTFLAGS="-C strip=symbols" SQLX_OFFLINE=true cargo build --locked --release --package website; \
    cp target/release/website ./main

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends libssl3 ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --system app && \
    useradd --system --gid app --home-dir /app --shell /usr/sbin/nologin app

COPY --from=build /build/website/Rocket.toml ./
COPY --from=build /build/main ./

ENV ROCKET_ADDRESS=::
ENV ROCKET_PORT=8000

EXPOSE 8000

USER app

CMD ["./main"]
