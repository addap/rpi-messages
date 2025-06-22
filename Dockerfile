FROM rust:1-bullseye AS builder
WORKDIR /app

# Copy common crate
#RUN mkdir common server
COPY ./common ./common

# Copy build files of server crate
COPY ./server/Cargo.toml ./server/Cargo.toml
COPY ./server/Cargo.lock ./server/Cargo.lock

RUN cd server && mkdir src && echo "fn main() {}" > src/main.rs && cargo install --path .

COPY ./server/src ./server/src
COPY ./server/pictures ./server/pictures
RUN cd server && touch src/main.rs && cargo install --path .

FROM debian:bullseye-slim

WORKDIR /app
RUN apt-get update && apt-get install -y openssl ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/server server
CMD ["/app/server"]
