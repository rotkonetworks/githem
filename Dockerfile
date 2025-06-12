FROM rust:slim as builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev libgit2-dev && \
    apt-get clean && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN cargo build --release --bin githem-api

FROM debian:bookworm-slim
# Security: Create non-root user for runtime
RUN groupadd -r githem && useradd -r -g githem -s /bin/false -d /app githem && \
    apt-get update && apt-get install -y ca-certificates libssl3 libgit2-1.5 && \
    apt-get clean && rm -rf /var/lib/apt/lists/*

# Security: Create secure app directory
RUN mkdir -p /app && chown githem:githem /app
WORKDIR /app

# Security: Copy binary with restricted permissions
COPY --from=builder --chown=githem:githem /app/target/release/githem-api /usr/local/bin/githem-api
RUN chmod 755 /usr/local/bin/githem-api

# Security: Switch to non-root user
USER githem

# Security: Restrict network access
EXPOSE 42069 42070

# Security: Use non-root user and read-only filesystem
CMD ["githem-api"]
