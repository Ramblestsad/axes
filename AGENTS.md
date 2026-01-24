# Repository Guidelines

## Project Structure & Module Organization
- `src/` contains the Axum HTTP service, gRPC server, handlers, routes, config, and utilities.
- `src/handlers/` hosts HTTP handlers grouped by domain (auth, users, bakery).
- `src/grpc/` contains gRPC service implementations; protobuf definitions live in `protos/`.
- `settings/` holds environment config templates; runtime config loads `./settings/<ENVIRONMENT>.toml`.
- `deployment.yaml`, `service.yaml`, and `Dockerfile` are for container and Kubernetes deployment.

## Build, Test, and Development Commands
- `cargo build`: compile the project.
- `cargo run`: run both HTTP and gRPC servers locally.
- `cargo test`: run unit tests (currently minimal/empty).
- `ENVIRONMENT=development cargo run`: use `settings/development.toml`.

## Coding Style & Naming Conventions
- Rust formatting is enforced via `rustfmt.toml` (max width 100, grouped imports).
- Prefer module names that mirror domains (`handlers`, `routes`, `grpc`).
- Follow conventional Rust naming: `snake_case` for functions/vars, `CamelCase` for types.

## Testing Guidelines
- Tests live under `src/tests.rs` and module-level `#[cfg(test)]` blocks.
- Use `cargo test` locally; add new tests alongside the module they cover.
- No formal coverage threshold is defined yet.

## Configuration & Environment
- HTTP and gRPC listen addresses are configured via `AXES_HTTP_ADDR` and `AXES_GRPC_ADDR`.
  - Defaults: `0.0.0.0:5173` (HTTP) and `0.0.0.0:5273` (gRPC).
- Config is loaded from `settings/<ENVIRONMENT>.toml` (see `settings/*.example.toml`).

## Commit Message Conventions
- Follow conventional commits seen in history: `feat:`, `fix:`, `refactor:`, `style:`, `build:`.
- Keep subjects short and action-oriented (e.g., `feat: add grpc router`).
