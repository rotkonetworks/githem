FROM rust:1-bullseye as builder
RUN apt-get update && \
    apt-get install -y \
        pkg-config \
        libssl-dev \
        libgit2-dev \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
ENV OPENSSL_NO_VENDOR=0
RUN cargo build --release --bin githem-api

FROM rust:1-slim-bullseye
RUN groupadd -r githem && \
    useradd -r -g githem -s /bin/false -d /app githem
WORKDIR /app
COPY --from=builder /app/target/release/githem-api /app/githem-api
RUN chown -R githem:githem /app
USER githem
EXPOSE 42069 42070
CMD ["/app/githem-api"]
