# Operations Runbook

## Services

| Service | Role | Host |
|---|---|---|
| cardguard-api | Axum HTTP server | Fly.io |
| cardguard-web | Leptos SSR web server | Fly.io |
| cardguard-api-db | PostgreSQL 16 | Fly.io Postgres |
| cardguard-redis | Redis (velocity counters + job queue) | Fly.io Redis |

## First-time setup

```bash
# Install Fly.io CLI
curl -L https://fly.io/install.sh | sh
fly auth login

# Create the apps
fly apps create cardguard-api
fly apps create cardguard-api-staging
fly apps create cardguard-web
fly apps create cardguard-web-staging

# Provision Postgres
fly postgres create --name cardguard-api-db --region iad
fly postgres attach cardguard-api-db --app cardguard-api

# Provision Redis
fly redis create --name cardguard-redis --region iad
# Note the Redis URL from the output

# Set secrets (production)
fly secrets set \
  JWT_SECRET=$(openssl rand -hex 32) \
  ENCRYPTION_KEY=$(openssl rand -hex 32) \
  REDIS_URL=<redis-url-from-above> \
  POKEMON_TCG_API_KEY=<optional> \
  PRICECHARTING_API_KEY=<optional> \
  PSA_API_KEY=<optional> \
  SMTP_HOST=<host> \
  SMTP_PORT=587 \
  SMTP_USERNAME=<user> \
  SMTP_PASSWORD=<pass> \
  APP_BASE_URL=https://api.cardguard.app \
  --app cardguard-api

# Set secrets (staging — use separate values)
fly secrets set \
  JWT_SECRET=$(openssl rand -hex 32) \
  ENCRYPTION_KEY=$(openssl rand -hex 32) \
  APP_BASE_URL=https://api.staging.cardguard.app \
  --app cardguard-api-staging
```

## Deploy

Deploys are triggered automatically by GitHub Actions CI when branches merge:
- `develop` → staging apps
- `main` → production apps

Manual deploy:
```bash
# Staging
fly deploy --app cardguard-api-staging --config fly.toml
fly deploy --app cardguard-web-staging --config deploy/web.fly.toml

# Production
fly deploy --app cardguard-api --config fly.toml
fly deploy --app cardguard-web --config deploy/web.fly.toml
```

## Database migrations

Applied automatically on server start via `sqlx::migrate!`.

Manual apply:
```bash
fly ssh console -a cardguard-api
# Inside the console:
sqlx migrate run --database-url "$DATABASE_URL"
```

Check migration status:
```bash
fly ssh console -a cardguard-api -C "sqlx migrate info --database-url \$DATABASE_URL"
```

## Rollback

```bash
# List recent deployments
fly releases --app cardguard-api

# Roll back to a specific version
fly deploy --image <image-ref> --app cardguard-api
```

## Database backup and restore

```bash
# Create a snapshot
fly postgres backup list --app cardguard-api-db
fly postgres backup create --app cardguard-api-db

# Restore (to staging)
fly postgres restore --restore-backup <backup-id> --app cardguard-api-staging-db
```

## Redis

Redis is used for:
- Velocity counters (fraud detection: cert scan frequency per buyer)
- Apalis job queue (background jobs: valuation snapshots, reconciliation)

If Redis is unavailable, velocity counters degrade gracefully (counters return 0).
Job queue requires Redis — background jobs pause until Redis is restored.

Check Redis health:
```bash
fly redis status cardguard-redis
```

## Monitoring

Key metrics to watch:
- API error rate (5xx responses) via Fly.io metrics
- Database connection pool saturation
- Migration lag on startup (check logs for `sqlx::migrate` messages)
- Redis connection errors (check for `REDIS__URL not set` warnings)

View logs:
```bash
fly logs --app cardguard-api
fly logs --app cardguard-web
```

## Environment variables reference

| Variable | Required | Description |
|---|---|---|
| `DATABASE_URL` | Yes | PostgreSQL connection string (auto-set by Fly Postgres) |
| `JWT_SECRET` | Yes | 32+ byte random secret for JWT signing |
| `ENCRYPTION_KEY` | Yes | 32-byte hex for AES-256-GCM column encryption |
| `REDIS_URL` | No | Redis connection string (velocity counters disabled if absent) |
| `APP_BASE_URL` | Yes | Base URL for email links |
| `SMTP_HOST` | No | SMTP server host (email disabled if absent) |
| `SMTP_PORT` | No | SMTP port (default: 587) |
| `SMTP_USERNAME` | No | SMTP credentials |
| `SMTP_PASSWORD` | No | SMTP credentials |
| `POKEMON_TCG_API_KEY` | No | Pokemon TCG API key (public rate limit without it) |
| `PRICECHARTING_API_KEY` | No | PriceCharting API key for valuations |
| `PSA_API_KEY` | No | PSA grading API key |
