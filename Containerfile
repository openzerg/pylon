FROM docker.io/oven/bun:latest AS builder

RUN apt-get update && apt-get install -y git && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY package.json bun.lock* ./
RUN bun install

COPY src/ src/
COPY tsconfig.json ./

FROM docker.io/library/busybox:glibc

WORKDIR /app

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /app/node_modules ./node_modules
COPY --from=builder /app/package.json ./
COPY --from=builder /app/src ./src
COPY --from=builder /app/tsconfig.json ./
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt
ENV PYLON_PORT=15316
ENV PYLON_DB=/tmp/pylon.db

EXPOSE 15316

ENTRYPOINT ["/entrypoint.sh"]
CMD ["bun", "run", "src/main.ts"]
