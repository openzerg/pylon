FROM docker.io/oven/bun:alpine AS builder

WORKDIR /app

COPY package.json bun.lock* ./
RUN bun install

COPY src/ src/
COPY tsconfig.json ./

FROM docker.io/library/alpine:3.23

RUN apk add --no-cache ca-certificates sqlite-libs bash

WORKDIR /app

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