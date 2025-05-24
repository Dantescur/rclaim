FROM rust:1.86 as builder
WORKDIR /usr/src/rclaim
COPY . .
RUN cargo build --release

FROM rust:1.86-slim
WORKDIR /app
COPY --from=builder /usr/src/rclaim/target/release/rclaim /app/rclaim
ENV RUST_LOG=info
ENV WS_AUTH_TOKEN=THE_SECRET_TOKEN
ENV PORT=8082
ENV HOST=0.0.0.0
CMD [ "/app/rclaim" ]
