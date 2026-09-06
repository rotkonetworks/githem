FROM rust:1-bookworm AS builder
RUN apt-get update && \
    apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
# libgit2 and openssl are built from the vendored sources so the
# runtime image needs no shared libraries beyond libc
ENV OPENSSL_NO_VENDOR=0
RUN cargo build --release --bin githem-api

FROM debian:bookworm-slim
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* && \
    groupadd -r githem && \
    useradd -r -g githem -s /bin/false -d /app githem
WORKDIR /app
COPY --from=builder /app/target/release/githem-api /app/githem-api
RUN chown -R githem:githem /app
USER githem
EXPOSE 42069 42070
CMD ["/app/githem-api"]
