FROM docker.io/rust:1-slim-bookworm AS build

WORKDIR /build

COPY . .

RUN apt update && apt install -y libssl-dev pkg-config

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=secret,id=DATABASE_URL \
    set -eux; \
    export DATABASE_URL=$(cat /run/secrets/DATABASE_URL); \
    cargo build --release --package website; \
    objcopy --compress-debug-sections target/release/website ./main

################################################################################

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN apt update && apt install -y libssl3 ca-certificates curl

COPY --from=build /build/website/Rocket.toml ./
## copy the main binary
COPY --from=build /build/main ./

COPY --from=build /build/website/static ./static
COPY --from=build /build/website/templates ./templates

## ensure the container listens globally on port 8080
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8080

EXPOSE 8080

CMD ./main
