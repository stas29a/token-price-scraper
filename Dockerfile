FROM rust:1.88 as builder

WORKDIR /usr/src/app

RUN apt-get update && apt-get install -y libpq-dev

COPY Cargo.toml Cargo.lock ./

COPY src ./src
COPY diesel.toml ./diesel.toml
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /usr/src/app/target/release/price-scraper /app/price-scraper
COPY static /app/static
COPY config.toml /app/config.toml

EXPOSE 8080
ENV RUST_LOG=debug

CMD ["./price-scraper"]