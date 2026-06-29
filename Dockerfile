# syntax=docker/dockerfile:1

################################################################################
# Create a stage for building the application.

ARG APP_NAME=axes
FROM rust:1-slim-bookworm AS build
# openssl issues workaround
RUN apt-get update -y && \
    apt-get install -y pkg-config make g++ libssl-dev && \
    rustup target add x86_64-unknown-linux-gnu

ARG APP_NAME
WORKDIR /app

ENV SQLX_OFFLINE=true

COPY . .

RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --release --bins && \
    cp ./target/release/$APP_NAME /bin/${APP_NAME} && \
    cp ./target/release/inventory-worker /bin/inventory-worker && \
    cp ./target/release/orders-worker /bin/orders-worker

FROM debian:bookworm-slim AS final

ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser

COPY --from=build /bin/axes /bin/inventory-worker /bin/orders-worker /bin/
RUN chown appuser /bin/axes /bin/inventory-worker /bin/orders-worker
RUN mkdir /settings && chown appuser /settings

USER appuser

ENV ENVIRONMENT=production
ENV RUST_LOG="axes=debug,info"
ENV AXES_HTTP_ADDR="0.0.0.0:7878"
ENV AXES_GRPC_ADDR="0.0.0.0:50051"

EXPOSE 7878 50051

CMD ["/bin/axes"]
