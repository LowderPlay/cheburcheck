# syntax=docker/dockerfile:1.10

FROM docker.io/rust:1-slim-bookworm AS build

WORKDIR /build

ENV PNPM_HOME=/pnpm
ENV PATH="${PNPM_HOME}:${PATH}"

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl libssl-dev pkg-config && \
    curl -fsSL https://deb.nodesource.com/setup_24.x | bash - && \
    apt-get install -y --no-install-recommends nodejs && \
    corepack enable && \
    corepack prepare pnpm@10.33.0 --activate && \
    pnpm config set store-dir /pnpm/store && \
    rm -rf /var/lib/apt/lists/*

COPY frontend/package.json frontend/pnpm-lock.yaml frontend/pnpm-workspace.yaml ./frontend/

RUN --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    pnpm --dir frontend fetch --frozen-lockfile

COPY . .

RUN --mount=type=cache,id=website-target,target=/build/target \
    --mount=type=cache,id=cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    set -eux; \
    pnpm --dir frontend install --frozen-lockfile --offline; \
    pnpm --dir frontend build; \
    RUSTFLAGS="-C strip=symbols" SKIP_FRONTEND_BUILD=true SQLX_OFFLINE=true cargo build --locked --release --package website; \
    cp target/release/website ./main

################################################################################

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && \
    apt-get install -y --no-install-recommends libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/* && \
    groupadd --system app && \
    useradd --system --gid app --home-dir /app --shell /usr/sbin/nologin app

COPY --from=build /build/website/Rocket.toml ./
## copy the main binary
COPY --from=build /build/main ./

## ensure the container listens globally on port 8080
ENV ROCKET_ADDRESS=::
ENV ROCKET_PORT=8080

EXPOSE 8080

USER app

CMD ["./main"]
