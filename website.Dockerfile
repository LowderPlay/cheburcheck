# syntax=docker/dockerfile:1

################################################################################
# Build Stage
################################################################################
FROM dhi.io/alpine-base:3.23-dev AS build

WORKDIR /build

ENV CARGO_NET_GIT_FETCH_WITH_CLI=true \
    CARGO_TERM_COLOR=always \
    CARGO_PROFILE_RELEASE_LTO=true \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_DEBUG=false \
    CARGO_PROFILE_RELEASE_STRIP=true \
    CARGO_PROFILE_RELEASE_DEBUG_ASSERTIONS=false \
    CARGO_PROFILE_RELEASE_OVERFLOW_CHECKS=false \
    CARGO_PROFILE_RELEASE_PANIC=abort \
    OPENSSL_STATIC=1 \
    SQLX_OFFLINE=true \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:$PATH

RUN apk add --no-cache \
    curl \
    libgcc \
    openssl-dev \
    openssl-libs-static \
    pkgconf \
    musl-dev \
    build-base \
    && rm -rf /var/cache/apk/*

RUN set -eux; \
    case "$(apk --print-arch)" in \
      x86_64)  ARCH=x86_64  ;; \
      aarch64) ARCH=aarch64 ;; \
      *) echo "unsupported arch: $(apk --print-arch)"; exit 1 ;; \
    esac; \
    curl -fsSL "https://static.rust-lang.org/rustup/dist/${ARCH}-unknown-linux-musl/rustup-init" -o /tmp/rustup-init; \
    chmod +x /tmp/rustup-init; \
    /tmp/rustup-init -y --default-toolchain stable --profile minimal; \
    rm /tmp/rustup-init; \
    rustup --version; \
    cargo --version; \
    rustc --version

COPY . .

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    set -eux; \
    cargo build \
      --release \
      --package website \
      --locked; \
    objcopy --compress-debug-sections target/release/website /tmp/main; \
    file /tmp/main

RUN cp /tmp/main /build/main

################################################################################
# Runtime Stage
################################################################################
FROM gcr.io/distroless/static:nonroot

LABEL org.opencontainers.image.title="Website" \
      org.opencontainers.image.description="Rocket-based website application" \
      org.opencontainers.image.version="1.0"

WORKDIR /app

COPY --from=build --chown=nonroot:nonroot \
    /build/main \
    /build/website/Rocket.toml \
    ./

COPY --from=build /build/website/static ./static
COPY --from=build /build/website/templates ./templates

ENV ROCKET_ADDRESS=::
ENV ROCKET_PORT=8080

EXPOSE 8080

ENTRYPOINT ["/app/main"]
