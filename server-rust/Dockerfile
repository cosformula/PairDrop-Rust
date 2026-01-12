# Build stage
FROM rust:1.83-alpine AS builder

RUN apk add --no-cache musl-dev

WORKDIR /app

# Copy manifests
COPY server-rust/Cargo.toml server-rust/Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source code
COPY server-rust/src ./src

# Build the actual binary
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM alpine:3.19

RUN apk add --no-cache ca-certificates

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/pairdrop-server /app/pairdrop-server

# Copy public directory (frontend)
COPY public ./public

# Expose port
EXPOSE 3000

ENV PORT=3000

# Run the server
CMD ["./pairdrop-server"]
