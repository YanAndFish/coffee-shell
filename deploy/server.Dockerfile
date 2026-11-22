# syntax=docker/dockerfile:1.7
#
# ROAST/TTY backend: @coffee/server (Koa + Prisma + SQLite), built and run with Node.
# Build from the repository root:
#
#   docker build -f deploy/server.Dockerfile -t roast-tty-server .
#
# deploy/server.Dockerfile.dockerignore keeps the context to the Node workspace.
# deploy/compose.yaml runs it next to the SSH gateway (deploy/Dockerfile).

ARG NODE_VERSION=26
ARG DEBIAN_RELEASE=bookworm
ARG PNPM_VERSION=12.9.1

FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS build
ARG PNPM_VERSION
# python3/make/g++: better-sqlite3 compiles its binding when no prebuilt one matches.
# openssl: Prisma picks the schema engine build for the system's OpenSSL.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates openssl python3 make g++ \
 && rm -rf /var/lib/apt/lists/* \
 && npm install --global --no-fund --no-audit "pnpm@${PNPM_VERSION}"
ENV CI=1 \
    CHECKPOINT_DISABLE=1 \
    PRISMA_HIDE_UPDATE_MESSAGE=1 \
    npm_config_store_dir=/pnpm-store
WORKDIR /src
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml tsconfig.base.json ./
COPY packages ./packages
COPY apps/server ./apps/server
# The whole workspace (tsdown comes from the root); `prisma generate` runs on install.
RUN --mount=type=cache,target=/pnpm-store \
    pnpm install --frozen-lockfile
# dist/main.mjs is the server; the seed is bundled the same way (with the contract
# inlined), because Node does not run TypeScript from node_modules.
RUN pnpm --filter @coffee/server build \
 && cd apps/server \
 && pnpm exec tsdown prisma/seed.ts --out-dir dist-seed
# The server's own dependencies, resolved from the lockfile, in a directory of their own:
# the runtime libraries plus the Prisma CLI (`migrate deploy` on start). It sits two levels
# below /src, like apps/server, so the package's tsconfig (`../../tsconfig.base.json`)
# still resolves when its postinstall (`prisma generate`) runs.
RUN --mount=type=cache,target=/pnpm-store \
    pnpm --filter @coffee/server deploy /src/.deploy/server

FROM node:${NODE_VERSION}-${DEBIAN_RELEASE}-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends openssl \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 coffee \
 && useradd --system --uid 10001 --gid coffee --home-dir /home/coffee --create-home \
      --shell /usr/sbin/nologin coffee \
 && install -d -o coffee -g coffee -m 0700 /data
WORKDIR /app
# Owned by root, read-only for the server's user.
COPY --from=build /src/.deploy/server/package.json ./
COPY --from=build /src/.deploy/server/node_modules ./node_modules
COPY --from=build /src/apps/server/dist/main.mjs /src/apps/server/dist-seed/seed.mjs ./dist/
COPY --from=build /src/apps/server/prisma/schema.prisma ./prisma/
COPY --from=build /src/apps/server/prisma/migrations ./prisma/migrations
COPY deploy/server/prisma.config.mjs ./
COPY --chmod=0755 deploy/server/start.sh ./

# HOST 0.0.0.0 is inside the container: compose keeps the port on a private network and
# never publishes it. COFFEE_INTERNAL_KEY has no default: give it at run time.
ENV NODE_ENV=production \
    HOST=0.0.0.0 \
    PORT=3000 \
    DATABASE_URL=file:/data/coffee.db \
    CHECKPOINT_DISABLE=1 \
    PRISMA_HIDE_UPDATE_MESSAGE=1 \
    LANG=C.UTF-8

# The SQLite database lives here: keep it across restarts.
VOLUME ["/data"]
USER coffee:coffee
EXPOSE 3000
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=5s --timeout=3s --start-period=30s --retries=6 \
  CMD ["node", "-e", "fetch('http://127.0.0.1:3000/v1/health').then(r => process.exit(r.ok ? 0 : 1), () => process.exit(1))"]
ENTRYPOINT ["/app/start.sh"]
