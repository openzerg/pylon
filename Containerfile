FROM nixos/nix:latest

WORKDIR /app

RUN nix-env -iA nixpkgs.bun nixpkgs.sqlite nixpkgs.bash nixpkgs.coreutils nixpkgs.cacert nixpkgs.curl

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt
ENV NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-bundle.crt
ENV NIX_SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt

COPY package.json bun.lock* ./
RUN SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-bundle.crt bun install

COPY src/ src/
COPY tsconfig.json ./

ENV PYLON_PORT=15316
ENV PYLON_DB=/tmp/pylon.db

EXPOSE 15316

CMD ["bun", "run", "src/main.ts"]