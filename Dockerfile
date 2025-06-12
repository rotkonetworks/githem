FROM rust:slim as builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev libgit2-dev
WORKDIR /app
COPY . .
RUN cargo build --release --bin githem-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 libgit2-1.5 curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/githem-api /usr/local/bin/
EXPOSE 42069 42070
CMD ["githem-api"]
