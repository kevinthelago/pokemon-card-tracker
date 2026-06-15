# Architecture

See [context/architecture.md](../../../context/architecture.md) in the plan hub for the full design doc.

## Quick reference

```
crates/
  domain/   — Pure shared types (no DB / web deps)
  api/      — Axum server: catalogue, inventory, valuation, grading, POS, fraud
  web/      — Leptos SSR web app
```

External services:
- Pokémon TCG API (pokemontcg.io) — card identity + prices
- PSA / CGC / BGS cert APIs — grading verification
- Square / Shopify / Clover — POS sync
- PriceCharting — fallback + graded prices
