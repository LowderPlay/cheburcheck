# syntax=docker/dockerfile:1.10

FROM docker.io/node:24-bookworm-slim AS build

WORKDIR /app

ENV PNPM_HOME=/pnpm
ENV PATH="${PNPM_HOME}:${PATH}"

RUN corepack enable && \
    corepack prepare pnpm@10.33.0 --activate && \
    pnpm config set store-dir /pnpm/store

COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./

RUN --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    pnpm fetch --frozen-lockfile

COPY . ./

RUN --mount=type=cache,id=pnpm-store,target=/pnpm/store \
    set -eux; \
    pnpm install --frozen-lockfile --offline; \
    pnpm build; \
    pnpm prune --prod

FROM docker.io/node:24-bookworm-slim

WORKDIR /app

ENV HOST=0.0.0.0
ENV PORT=3000
ENV NODE_ENV=production

COPY --from=build /app/build ./build
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/package.json ./package.json

EXPOSE 3000

USER node

CMD ["node", "build/index.js"]
