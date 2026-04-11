FROM docker.io/nixos/nix:latest AS builder

RUN nix --extra-experimental-features "nix-command flakes" --option substituters "https://mirrors.ustc.edu.cn/nix-channels/store https://cache.nixos.org/" --option trusted-public-keys "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=" profile install github:NixOS/nixpkgs/nixos-25.11#bun

ENV PATH=/nix/profiles/default/bin:${PATH}

WORKDIR /app

COPY package.json bun.lock* ./
RUN bun install

COPY src/ src/
COPY tsconfig.json ./

RUN bun build --compile src/main.ts --outfile pylon

FROM docker.io/library/debian:trixie-slim

WORKDIR /app

COPY --from=builder /app/pylon /app/pylon

RUN cat > /entrypoint.sh << 'EOF'
#!/bin/sh
set -e

mkdir -p /root

mkdir -p /etc/ssl/certs
for cert in /nix/store/*-nss-cacert-*/etc/ssl/certs/ca-bundle.crt; do
    if [ -f "$cert" ]; then
        ln -sf "$cert" /etc/ssl/certs/ca-certificates.crt
        export SSL_CERT_FILE="$cert"
        export CURL_CA_BUNDLE="$cert"
        export GIT_SSL_CAINFO="$cert"
        export NIX_SSL_CERT_FILE="$cert"
        break
    fi
done

export NIX_REMOTE=daemon
export NIX_CONFIG="experimental-features = nix-command flakes
flake-registry = /tmp/nix-registry.json"
export XDG_CACHE_HOME=/nix-cache

NIX_BIN=$(ls -d /nix/store/*-nix-2.* 2>/dev/null | grep -v '\.drv$' | grep -v '\.patch$' | head -1)
if [ -n "$NIX_BIN" ]; then
    export PATH="${NIX_BIN}/bin:${PATH}"
fi

mkdir -p "${PYLON_DB%/*}" 2>/dev/null || true

exec "$@"
EOF
RUN chmod +x /app/pylon /entrypoint.sh

ENV PYLON_PORT=15316
ENV PYLON_DB=/var/lib/pylon/pylon.db

EXPOSE 15316

ENTRYPOINT ["/entrypoint.sh"]
CMD ["./pylon"]
