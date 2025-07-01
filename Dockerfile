FROM rust:1-bullseye AS builder
WORKDIR /build

# Copy common crate
#RUN mkdir common server
COPY ./common ./common

# Copy build files of server crate
COPY ./server/Cargo.toml ./server/Cargo.toml
COPY ./server/Cargo.lock ./server/Cargo.lock

# Create fake main.rs to be able to compile crate
RUN cd server && mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release

# Copy the actual sources to compile
COPY ./server/src ./server/src
COPY ./server/pictures ./server/pictures
RUN cd server && touch src/main.rs && cargo build --release

FROM debian:bullseye-slim

WORKDIR /app
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/server/target/release/server server
CMD ["/app/server"]
