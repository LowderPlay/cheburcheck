FROM docker.io/rust:1-slim-bookworm AS build

WORKDIR /build

COPY . .

RUN apt update && apt install -y ca-certificates curl libssl-dev pkg-config && \
    curl -fsSL https://deb.nodesource.com/setup_24.x | bash - && \
    apt install -y nodejs && \
    corepack enable && \
    corepack prepare pnpm@10.33.0 --activate && \
    rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    export SQLX_OFFLINE=true; \
    cargo build --release --package website; \
    objcopy --compress-debug-sections target/release/website ./main

################################################################################

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN apt update && apt install -y libssl3 ca-certificates curl

COPY --from=build /build/website/Rocket.toml ./
## copy the main binary
COPY --from=build /build/main ./

## ensure the container listens globally on port 8080
ENV ROCKET_ADDRESS=::
ENV ROCKET_PORT=8080

EXPOSE 8080

CMD ./main
