# CardGuard — Pokémon Card Catalogue & Fraud Defence

A POS-synced card catalogue and fraud-defence tool for Pokémon card sellers and collectors. Built entirely in Rust (Axum API + Leptos SSR web + shared domain types).

## Crate layout

```
crates/
  domain/   Pure shared types (Printing, CardInstance, InventoryItem, …). No DB or web deps.
  api/      Axum HTTP server: catalogue, inventory, valuation, grading, POS sync, fraud engine.
  web/      Leptos SSR web app: card scanning UI, inventory browser, risk dashboard.
```

## Prerequisites

- Rust 1.87+ (`rustup show` — the `rust-toolchain.toml` auto-installs the right toolchain)
- `wasm32-unknown-unknown` target (`rustup target add wasm32-unknown-unknown`)
- Docker (for integration tests via testcontainers and for local Postgres/Redis)
- [Tailwind CSS CLI](https://tailwindcss.com/docs/installation) v4 (for web asset compilation)
- `cargo-audit` and `cargo-deny` for supply-chain checks

Optional (for Leptos live-reload):
- `cargo-leptos` (`cargo install cargo-leptos`)

## Environment variables

Copy `.env.example` to `.env` and fill in the values:

```
DATABASE_URL=postgres://postgres:password@localhost:5432/cardguard
REDIS_URL=redis://localhost:6379
JWT_SECRET=<32-byte random hex>
ENCRYPTION_KEY=<32-byte random hex for AES-256-GCM column encryption>
POKEMON_TCG_API_KEY=<optional — public rate limit without key>
PRICECHARTING_API_KEY=<optional>
PSA_API_KEY=<optional>
```

## Build

```bash
# Check everything compiles
cargo check --workspace

# Run the API server (development)
cargo run -p api

# Run the web server with SSR (development)
cargo run -p web --features ssr

# Build the WASM client bundle (run before serving web)
cargo build -p web --lib --target wasm32-unknown-unknown --features hydrate

# Compile Tailwind CSS
npx tailwindcss -i crates/web/style/input.css -o crates/web/style/output.css
```

## Test

```bash
# Full test suite (Docker required for integration tests)
cargo test --workspace

# Unit tests only (no Docker)
cargo test --workspace --lib
```

## Lint & supply-chain

```bash
cargo fmt --all --check
cargo clippy --workspace --all-features -- -D warnings
cargo audit
cargo deny check
```

## Database migrations

SQLx migrations live in `crates/api/migrations/`. The API server applies them on startup with `sqlx::migrate!`.

To apply manually:
```bash
sqlx migrate run --database-url "$DATABASE_URL"
```

## Deployment

See `deploy/` for Fly.io configuration and the ops runbook.
