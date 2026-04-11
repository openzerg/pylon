FROM docker.io/nixos/nix:latest AS builder

RUN nix --extra-experimental-features "nix-command flakes" --option substituters "https://mirrors.ustc.edu.cn/nix-channels/store https://cache.nixos.org/" --option trusted-public-keys "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY=" profile install github:NixOS/nixpkgs/nixos-25.11#bun

ENV PATH=/nix/profiles/default/bin:${PATH}

WORKDIR /app

COPY package.json bun.lock* ./
RUN bun install

COPY src/ src/
COPY tsconfig.json ./

FROM docker.io/library/debian:trixie-slim

WORKDIR /app

COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./
COPY --from=builder /app/src ./src
COPY --from=builder /app/tsconfig.json ./
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV PYLON_PORT=15316
ENV PYLON_DB=/tmp/pylon.db

EXPOSE 15316

ENTRYPOINT ["/entrypoint.sh"]
CMD ["bun", "run", "src/main.ts"]
