FROM rust:1-alpine AS builder

WORKDIR /app

RUN apk add --no-cache ca-certificates build-base

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src

RUN cargo build --release

FROM alpine:3.22

RUN apk add --no-cache ca-certificates

COPY --from=builder /app/target/release/imap-idle-webhook /usr/local/bin/imap-idle-webhook

USER 65532:65532

ENTRYPOINT ["/usr/local/bin/imap-idle-webhook"]
