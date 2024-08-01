# syntax=docker/dockerfile:1

################################################################################
# Create a stage for building the application.

ARG RUST_VERSION=1.80.0
ARG APP_NAME=axes
FROM rust:${RUST_VERSION}-slim-bullseye AS build
# openssl issues workaround
RUN apt-get update -y && \
    apt-get install -y pkg-config make g++ libssl-dev && \
    rustup target add x86_64-unknown-linux-gnu
ARG APP_NAME
WORKDIR /app
COPY . .

RUN --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --release && \
    cp ./target/release/$APP_NAME /bin/${APP_NAME}

FROM debian:bullseye-slim AS final

ARG UID=10001
RUN adduser \
    --disabled-password \
    --gecos "" \
    --home "/nonexistent" \
    --shell "/sbin/nologin" \
    --no-create-home \
    --uid "${UID}" \
    appuser

COPY --from=build /bin/axes /bin/axes
RUN chown appuser /bin/axes
COPY --from=build /app/settings /settings
RUN chown -R appuser /settings

USER appuser

ENV ENVIRONMENT=production
ENV RUST_LOG="axes=debug,info"

EXPOSE 7878

CMD ["/bin/axes"]
