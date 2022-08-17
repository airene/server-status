FROM rust:1-alpine3.16 as builder
# This is important, see https://github.com/rust-lang/docker-rust/issues/85
ENV RUSTFLAGS="-C target-feature=-crt-static"
ENV RUST_BACKTRACE=1

WORKDIR /app
COPY ./ /app

RUN apk add --no-cache musl-dev git cmake make g++ protoc protobuf-dev
RUN cargo build --release --bin stat_server
RUN strip /app/target/release/stat_server

FROM alpine:3.16 as production
LABEL name=airene url=https://github.com/airene/server-status

RUN apk add --no-cache libgcc
COPY --from=builder /app/target/release/stat_server /app/stat_server

EXPOSE 9879 9880
ENTRYPOINT ["/app/stat_server"]
WORKDIR /root
CMD ["/app/stat_server","-c", "config.toml"]
