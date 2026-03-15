# Repository Guidelines

Please refer to me as "哥哥" in your responses.

## Project Structure & Module Organization
- `src/main.rs` is the main runtime entry for the HTTP and gRPC servers; start here for boot flow.
- `src/lib.rs` declares the top-level modules; use it to quickly map the crate surface area.
- `src/route.rs` wires HTTP routes; `src/handlers/` contains domain handlers grouped by feature.
- `src/grpc/` contains gRPC service implementations; protobuf definitions live in `protos/`.
- `src/orders/` and `src/bin/` contain worker-related logic and standalone binaries.
- `src/utils/observability/` contains logging, tracing, metrics, and OTLP setup; preserve existing patterns here.
- `src/tests/` contains integration-style module tests grouped by domain.
- `settings/` holds environment config templates; runtime config loads `./settings/<ENVIRONMENT>.toml`.
- `deployment.yaml`, `service.yaml`, and `Dockerfile` are for container and Kubernetes deployment.

## Build, Test, and Development Commands
- `cargo build`: compile the project.
- `cargo fmt --all`: format the workspace before finalizing Rust changes.
- `cargo run`: run both HTTP and gRPC servers locally.
- `cargo run --bin inventory-worker`: run the inventory worker binary.
- `cargo run --bin orders-worker`: run the orders worker binary.
- `cargo test`: run the test suite.
- `cargo test <filter>`: run the narrowest relevant tests first.
- `ENVIRONMENT=development cargo run`: use `settings/development.toml`.

## Coding Style & Naming Conventions
- Rust formatting is enforced via `rustfmt.toml` (max width 100, grouped imports).
- Prefer module names that mirror domains (`handlers`, `routes`, `grpc`).
- Follow conventional Rust naming: `snake_case` for functions/vars, `CamelCase` for types.
- Follow existing Axum, Tokio, and SQLx patterns already present in the repository before introducing new structure.
- Reuse existing error, observability, and configuration helpers unless there is a concrete reason not to.

## Efficient Working Defaults
- Start code exploration with `Cargo.toml`, `src/main.rs`, `src/lib.rs`, and then the feature-specific module being changed.
- For HTTP changes, inspect `src/route.rs`, the relevant handler module, `src/error.rs`, and the matching tests in `src/tests/`.
- For gRPC changes, inspect `src/grpc/`, the relevant proto in `protos/`, and any startup wiring in `src/main.rs`.
- For worker or async flow changes, inspect `src/orders/`, `src/bin/`, and shared shutdown/observability utilities.
- Prefer `rg` / `rg --files` for navigation and discovery.
- Default to the smallest viable change that fits the existing code shape.
- Prefer extending an existing module over creating a new top-level module unless the responsibility is clearly distinct.
- State assumptions briefly and continue unless the risk of being wrong is material.

## Change Strategy
- Do not over-abstract or over-engineer.
- Do not introduce a new trait, helper, wrapper, or module for hypothetical future reuse.
- Add abstraction only when at least two concrete call sites already need it, or when current duplication is already hurting readability or correctness.
- Favor straightforward, local code over indirection.
- Keep files focused, but do not split files purely for aesthetics.
- Prefer established crates already in `Cargo.toml` over adding new dependencies.
- When adding dependencies, choose mature crates with a clear need and keep the dependency surface small.

## Testing Guidelines
- Tests live under `src/tests/mod.rs`, `src/tests/*.rs`, and module-level `#[cfg(test)]` blocks.
- Use `cargo test` locally; add new tests alongside the module they cover.
- No formal coverage threshold is defined yet.
- For behavior changes, run the narrowest relevant test filter first, then broaden only as needed.
- For handler or route changes, prefer updating or adding the closest domain tests in `src/tests/`.
- For config, startup, or wiring changes, run `ENVIRONMENT=development cargo run` when practical.
- If you could not run a relevant verification command, say so explicitly and explain why.

## Configuration & Environment
- HTTP and gRPC listen addresses are configured via `AXES_HTTP_ADDR` and `AXES_GRPC_ADDR`.
  - Defaults: `0.0.0.0:5173` (HTTP) and `0.0.0.0:5273` (gRPC).
- Config is loaded from `settings/<ENVIRONMENT>.toml` (see `settings/*.example.toml`).
- Treat `settings/*.example.toml` as templates; avoid assuming secrets or machine-specific values live in the repo.

## Response Expectations
- Be concise, concrete, and action-oriented.
- Reference exact files and commands when they help the user verify or continue the work.
- For reviews, lead with bugs, regressions, and missing tests before summaries.
- Prefer doing the work over proposing large plans unless the user asks for planning first.

## Commit Message Conventions
- Follow conventional commits seen in history: `feat:`, `fix:`, `refactor:`, `style:`, `build:`.
- Keep subjects short and action-oriented (e.g., `feat: add grpc router`).
