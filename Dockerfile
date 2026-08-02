# Multi-stage build: compile the release binary, then ship it in a slim runtime image.
# Templates (Askama) and the vendored htmx.min.js are compiled/embedded into the binary at
# build time, so the runtime image only needs the binary itself plus ffmpeg (audio conversion)
# and TLS certs (reqwest's TLS handshake to DLSite happens end-to-end through gluetun's proxy —
# gluetun only relays bytes, it doesn't terminate TLS, so this image still needs its own trust
# store).

FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
        ffmpeg \
        ca-certificates \
        libssl3 \
        rsync \
        openssh-client \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/hvtag /usr/local/bin/hvtag

# hvtag has no CLI flag for its config/db location — it always reads ~/.hvtag/. Running as root
# keeps that simple (HOME=/root); harden with a dedicated user + HOME override later if desired.
ENTRYPOINT ["hvtag"]
CMD ["--ui", "--ui-bind", "0.0.0.0:8787"]
