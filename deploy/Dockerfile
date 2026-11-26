# syntax=docker/dockerfile:1.7
#
# ROAST/TTY SSH gateway: tty-gateway plus the coffee-tui it starts for every session,
# configured by deploy/gateway.toml.
# Build from the repository root:
#
#   docker build -f deploy/Dockerfile -t roast-tty-ssh .
#
# deploy/Dockerfile.dockerignore keeps target/, node_modules/ and the rest out of the
# build context (BuildKit reads it because it sits next to this file).

ARG RUST_VERSION=1.99.1
ARG DEBIAN_RELEASE=bookworm

FROM rust:${RUST_VERSION}-${DEBIAN_RELEASE} AS build
WORKDIR /src
# Only what the Rust workspace needs: its manifests, the TUI app (with theme.json),
# the crates (every workspace member, tty-gateway-testkit included), and the contract
# fixtures coffee-api's tests read.
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY apps/tui ./apps/tui
COPY crates ./crates
COPY packages/contract/fixtures ./packages/contract/fixtures
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p tty-gateway -p coffee-tui \
 && install -D -m 0755 target/release/tty-gateway /out/tty-gateway \
 && install -D -m 0755 target/release/coffee-tui /out/coffee-tui

FROM debian:${DEBIAN_RELEASE}-slim AS runtime
# ca-certificates: coffee-tui speaks HTTPS when COFFEE_API_URL is https://.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && groupadd --system --gid 10001 coffee \
 && useradd --system --uid 10001 --gid coffee --home-dir /home/coffee --create-home \
      --shell /usr/sbin/nologin coffee \
 && install -d -o coffee -g coffee -m 0700 /data
COPY --from=build /out/tty-gateway /out/coffee-tui /usr/local/bin/

# The gateway's configuration: the shop (default route) and gallery@. Mount your own over it
# (read-only); `docker kill -s HUP` reloads it. coffee-tui is found next to tty-gateway.
COPY deploy/gateway.toml /etc/tty-gateway/gateway.toml

# TTY_GATEWAY_* override the file's listen address and host key. The shop's route reads
# COFFEE_API_URL, COFFEE_INTERNAL_KEY (never baked in: compose passes it) and COFFEE_COLORS.
ENV TTY_GATEWAY_CONFIG=/etc/tty-gateway/gateway.toml \
    TTY_GATEWAY_ADDR=0.0.0.0:2222 \
    TTY_GATEWAY_HOST_KEY=/data/ssh_host_ed25519_key \
    COFFEE_API_URL=http://127.0.0.1:3000 \
    LANG=C.UTF-8

# The host key lives here: keep it across restarts, or clients see a new host key.
VOLUME ["/data"]
USER coffee:coffee
WORKDIR /home/coffee
EXPOSE 2222
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/tty-gateway"]
