# Build stage
FROM rust:1-slim AS build
WORKDIR /app

# Cache dependencies in a separate layer.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=build /app/target/release/middlegame-trainer-backend /usr/local/bin/middlegame-trainer-backend

ENV BIND=0.0.0.0:8080 \
    DATABASE_URL=sqlite://data/app.db \
    STATIC_DIR=/app/static
# The frontend build is mounted/copied to /app/static at deploy time (see docker-compose.yml).
VOLUME ["/app/data", "/app/static"]

EXPOSE 8080
CMD ["middlegame-trainer-backend"]
