# syntax=docker/dockerfile:1.7
#
# ROAST/TTY SSH gateway: coffee-sshd plus the coffee-tui it starts for every session.
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
# the crates, and the contract fixtures coffee-api's tests read.
COPY Cargo.toml Cargo.lock rust-toolchain.toml rustfmt.toml ./
COPY apps/tui ./apps/tui
COPY crates ./crates
COPY packages/contract/fixtures ./packages/contract/fixtures
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p coffee-sshd -p coffee-host \
 && install -D -m 0755 target/release/coffee-sshd /out/coffee-sshd \
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
COPY --from=build /out/coffee-sshd /out/coffee-tui /usr/local/bin/

ENV COFFEE_SSH_ADDR=0.0.0.0:2222 \
    COFFEE_SSH_HOST_KEY=/data/ssh_host_ed25519_key \
    COFFEE_TUI_BIN=/usr/local/bin/coffee-tui \
    COFFEE_API_URL=http://127.0.0.1:3000 \
    LANG=C.UTF-8

# The host key lives here: keep it across restarts, or clients see a new host key.
VOLUME ["/data"]
USER coffee:coffee
WORKDIR /home/coffee
EXPOSE 2222
STOPSIGNAL SIGTERM
ENTRYPOINT ["/usr/local/bin/coffee-sshd"]
