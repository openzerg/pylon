FROM docker.io/oven/bun:1-alpine

WORKDIR /app

RUN apk add --no-cache sqlite bash coreutils ca-certificates curl

ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt
ENV NODE_EXTRA_CA_CERTS=/etc/ssl/certs/ca-certificates.crt

COPY package.json bun.lock* ./
RUN bun install --frozen-lockfile

COPY src/ src/
COPY tsconfig.json ./

ENV PYLON_PORT=15316
ENV PYLON_DB=/tmp/pylon.db

EXPOSE 15316

CMD ["bun", "run", "src/main.ts"]