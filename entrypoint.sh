#!/bin/sh
set -e

mkdir -p /root

export NIX_REMOTE=daemon
export NIX_CONFIG="experimental-features = nix-command flakes
flake-registry = /tmp/nix-registry.json"
export XDG_CACHE_HOME=/nix-cache

NIX_BIN=$(ls -d /nix/store/*-nix-2.* 2>/dev/null | grep -v '\.drv$' | grep -v '\.patch$' | head -1)
if [ -n "$NIX_BIN" ]; then
    export PATH="${NIX_BIN}/bin:${PATH}"
fi

NIXPKGS="nixpkgs"

export PATH="$($NIX_BIN/bin/nix path-info ${NIXPKGS}#bun)/bin:${PATH}"

exec "$@"