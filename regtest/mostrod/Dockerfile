# mostrod from the official release binary, pinned and checksum-verified
# (sha256 from the release's manifest.txt, signed by the maintainers).
FROM debian:bookworm-slim

ARG MOSTROD_VERSION=v0.19.3
ARG MOSTROD_SHA256=db5113f94ad4e9f63cba168695247454dbad2a4dabc20b11787ce1c8774afcb1

RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL -o /usr/local/bin/mostrod \
      "https://github.com/MostroP2P/mostro/releases/download/${MOSTROD_VERSION}/mostrod-x86_64-unknown-linux-musl" \
  && echo "${MOSTROD_SHA256}  /usr/local/bin/mostrod" | sha256sum -c - \
  && chmod +x /usr/local/bin/mostrod

# settings.toml is bind-mounted read-only at /config; mostrod keeps it next to
# its database, so it is copied into the data volume on every start.
CMD ["sh", "-c", "cp /config/settings.toml /data/settings.toml && exec mostrod -d /data"]
