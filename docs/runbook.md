# Operations Runbook

> TODO (H2): document staging/production deploy procedure, rollback steps,
> database backup/restore, Redis failover, and monitoring alerts.

## Services

| Service | Role |
|---|---|
| Fly.io (API) | Axum HTTP server |
| Fly.io Postgres | Primary datastore |
| Fly.io Redis | Velocity counters + job queue |

## Deploy

```bash
# Staging (auto-triggered on push to develop)
fly deploy --app cardguard-api-staging

# Production (auto-triggered on push to main)
fly deploy --app cardguard-api
```

## Database migrations

Applied automatically on server start via `sqlx::migrate!`.

Manual apply:
```bash
fly ssh console -a cardguard-api
sqlx migrate run --database-url "$DATABASE_URL"
```

## Environment secrets

```bash
fly secrets set JWT_SECRET=<secret> --app cardguard-api
fly secrets set ENCRYPTION__KEY_HEX=<hex> --app cardguard-api
fly secrets set DATABASE__URL=<url> --app cardguard-api
```
