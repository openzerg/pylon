#!/bin/bash
set -e

mkdir -p /root

# ── Nix setup ────────────────────────────────────────────────────────────────
if [ -d /nix/store ]; then
    NIX_PKG=$(ls /nix/store | grep -E '^[a-z0-9]+-bun-[0-9]+\.[0-9]+\.[0-9]+$' | sort | tail -1)
    if [ -n "$NIX_PKG" ]; then
        export PATH="/nix/store/${NIX_PKG}/bin:${PATH}"
    fi
fi

exec "$@"