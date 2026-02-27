#!/usr/bin/env python3
"""Generate synthetic demo data for find-anything README/marketing screenshots."""

import io
import os
import struct

ROOT = "/tmp/find-demo"

files = {}

# ── taskflow: fictional Rust task-management API ──────────────────────────────

files["projects/taskflow/README.md"] = """\
# taskflow

A lightweight task-management REST API built with Axum and SQLite.

## Features

- JWT-based authentication with refresh tokens
- Rate limiting per user (configurable)
- Webhook notifications on task state changes
- Role-based access control (admin, member, viewer)
- Full-text search across task titles and descriptions

## Quick start

```bash
cargo build --release
./target/release/taskflow --config config/default.toml
```

## Configuration

See `config/default.toml` for all options. Key settings:

- `auth.jwt_secret` — secret used to sign tokens (use a long random string)
- `auth.token_expiry_secs` — access token lifetime (default: 3600)
- `rate_limit.requests_per_minute` — per-user API rate limit
- `cache.ttl_secs` — in-memory cache TTL for task list queries

## Deployment

See `docs/deployment.md` for systemd unit, reverse proxy, and database
backup configuration.
"""

files["projects/taskflow/Cargo.toml"] = """\
[package]
name = "taskflow"
version = "0.4.2"
edition = "2021"

[[bin]]
name = "taskflow"
path = "src/main.rs"

[dependencies]
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
jsonwebtoken = "9"
bcrypt = "0.15"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1"
thiserror = "1"
reqwest = { version = "0.11", features = ["json"], optional = true }

[features]
webhooks = ["reqwest"]
"""

files["projects/taskflow/config/default.toml"] = """\
[server]
host = "0.0.0.0"
port = 8080

[database]
path = "data/taskflow.db"
max_connections = 5

[auth]
jwt_secret = "change-me-in-production"
token_expiry_secs = 3600
refresh_expiry_secs = 604800
bcrypt_cost = 12
password_min_length = 12

[rate_limit]
enabled = true
requests_per_minute = 60
burst = 10
# Exempted from rate limiting
admin_bypass = true

[cache]
enabled = true
ttl_secs = 30
max_entries = 1000

[webhooks]
enabled = true
timeout_secs = 10
retry_attempts = 3
retry_delay_secs = 5

[logging]
level = "info"
format = "json"
"""

files["projects/taskflow/src/main.rs"] = """\
use anyhow::Result;
use axum::{middleware, Router};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

mod auth;
mod cache;
mod db;
mod tasks;
mod api {
    pub mod middleware;
    pub mod routes;
}

use crate::api::middleware::{rate_limit_layer, require_auth};

#[derive(Clone)]
pub struct AppState {
    pub db: db::Pool,
    pub config: Arc<Config>,
    pub cache: cache::Cache,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("taskflow=debug,tower_http=info")
        .init();

    let config = Arc::new(Config::load("config/default.toml")?);
    let db = db::connect(&config.database.path).await?;
    db::migrate(&db).await?;

    let cache = cache::Cache::new(config.cache.max_entries, config.cache.ttl_secs);
    let state = AppState { db, config: config.clone(), cache };

    let app = Router::new()
        .merge(api::routes::router())
        .layer(middleware::from_fn_with_state(state.clone(), require_auth))
        .layer(rate_limit_layer(config.rate_limit.requests_per_minute))
        .with_state(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    info!("taskflow listening on {addr}");
    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
"""

files["projects/taskflow/src/auth.rs"] = """\
use anyhow::{bail, Result};
use bcrypt::{hash, verify, DEFAULT_COST};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const PASSWORD_MIN_LENGTH: usize = 12;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,   // user id
    pub role: String,
    pub exp: u64,
    pub iat: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64,
}

pub fn hash_password(password: &str) -> Result<String> {
    if password.len() < PASSWORD_MIN_LENGTH {
        bail!("password must be at least {PASSWORD_MIN_LENGTH} characters");
    }
    Ok(hash(password, DEFAULT_COST)?)
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    Ok(verify(password, hash)?)
}

pub fn issue_tokens(user_id: &str, role: &str, secret: &str, expiry_secs: u64) -> Result<TokenPair> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.to_string(),
        exp: now + expiry_secs,
        iat: now,
    };
    let access_token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    // Refresh token: longer-lived, same structure
    let refresh_claims = Claims { exp: now + expiry_secs * 24, ..claims };
    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(TokenPair { access_token, refresh_token, expires_in: expiry_secs })
}

pub fn validate_token(token: &str, secret: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )?;
    Ok(data.claims)
}

pub fn require_role(claims: &Claims, required: &str) -> Result<()> {
    if claims.role != required && claims.role != "admin" {
        bail!("insufficient permissions: need '{required}', have '{}'", claims.role);
    }
    Ok(())
}
"""

files["projects/taskflow/src/tasks.rs"] = """\
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Task {
    pub id: i64,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub assignee_id: Option<i64>,
    pub project_id: i64,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTask {
    pub title: String,
    pub description: Option<String>,
    pub assignee_id: Option<i64>,
    pub due_date: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateTask {
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub assignee_id: Option<i64>,
    pub due_date: Option<String>,
}

pub async fn list_tasks(db: &SqlitePool, project_id: i64) -> Result<Vec<Task>> {
    let tasks = sqlx::query_as!(
        Task,
        "SELECT * FROM tasks WHERE project_id = ? ORDER BY created_at DESC",
        project_id
    )
    .fetch_all(db)
    .await?;
    Ok(tasks)
}

pub async fn create_task(db: &SqlitePool, project_id: i64, req: CreateTask) -> Result<Task> {
    let task = sqlx::query_as!(
        Task,
        r#"
        INSERT INTO tasks (title, description, status, assignee_id, project_id, due_date,
                           created_at, updated_at)
        VALUES (?, ?, 'open', ?, ?, ?, datetime('now'), datetime('now'))
        RETURNING *
        "#,
        req.title, req.description, req.assignee_id, project_id, req.due_date
    )
    .fetch_one(db)
    .await?;
    Ok(task)
}

pub async fn update_task(db: &SqlitePool, id: i64, req: UpdateTask) -> Result<Option<Task>> {
    let task = sqlx::query_as!(
        Task,
        r#"
        UPDATE tasks SET
            title       = COALESCE(?, title),
            description = COALESCE(?, description),
            status      = COALESCE(?, status),
            assignee_id = COALESCE(?, assignee_id),
            due_date    = COALESCE(?, due_date),
            updated_at  = datetime('now')
        WHERE id = ?
        RETURNING *
        "#,
        req.title, req.description, req.status, req.assignee_id, req.due_date, id
    )
    .fetch_optional(db)
    .await?;
    Ok(task)
}

pub async fn search_tasks(db: &SqlitePool, project_id: i64, query: &str) -> Result<Vec<Task>> {
    let pattern = format!("%{query}%");
    let tasks = sqlx::query_as!(
        Task,
        r#"
        SELECT * FROM tasks
        WHERE project_id = ?
          AND (title LIKE ? OR description LIKE ?)
        ORDER BY updated_at DESC
        "#,
        project_id, pattern, pattern
    )
    .fetch_all(db)
    .await?;
    Ok(tasks)
}
"""

files["projects/taskflow/src/api/routes.rs"] = """\
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::Deserialize;

use crate::{
    auth::{hash_password, issue_tokens, verify_password},
    tasks::{self, CreateTask, UpdateTask},
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login",          post(login))
        .route("/auth/refresh",        post(refresh_token))
        .route("/projects",            get(list_projects).post(create_project))
        .route("/projects/:id",        get(get_project).put(update_project).delete(delete_project))
        .route("/projects/:id/tasks",  get(list_tasks).post(create_task))
        .route("/tasks/:id",           get(get_task).put(update_task).delete(delete_task))
        .route("/tasks/:id/complete",  post(complete_task))
        .route("/webhooks",            get(list_webhooks).post(register_webhook))
        .route("/webhooks/:id",        delete(delete_webhook))
        .route("/admin/users",         get(list_users))
        .route("/admin/rate-limits",   get(get_rate_limit_stats).post(reset_rate_limit))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let user = match db::get_user_by_name(&state.db, &req.username).await {
        Ok(Some(u)) => u,
        _ => return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response(),
    };
    match verify_password(&req.password, &user.password_hash) {
        Ok(true) => {}
        _ => return (StatusCode::UNAUTHORIZED, "invalid credentials").into_response(),
    }
    let tokens = issue_tokens(&user.id.to_string(), &user.role,
                               &state.config.auth.jwt_secret,
                               state.config.auth.token_expiry_secs).unwrap();
    Json(tokens).into_response()
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    if let Some(q) = params.q {
        match tasks::search_tasks(&state.db, project_id, &q).await {
            Ok(results) => Json(results).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        match tasks::list_tasks(&state.db, project_id).await {
            Ok(results) => Json(results).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    }
}

async fn create_task(
    State(state): State<AppState>,
    Path(project_id): Path<i64>,
    Json(req): Json<CreateTask>,
) -> impl IntoResponse {
    match tasks::create_task(&state.db, project_id, req).await {
        Ok(task) => (StatusCode::CREATED, Json(task)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn complete_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> impl IntoResponse {
    let req = UpdateTask { status: Some("done".into()), ..Default::default() };
    match tasks::update_task(&state.db, id, req).await {
        Ok(Some(task)) => {
            // Fire webhook asynchronously
            tokio::spawn(webhooks::notify_task_completed(state.clone(), task.clone()));
            Json(task).into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

async fn get_rate_limit_stats(State(state): State<AppState>) -> impl IntoResponse {
    Json(state.rate_limiter.stats()).into_response()
}

async fn reset_rate_limit(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> impl IntoResponse {
    state.rate_limiter.reset(&user_id);
    StatusCode::NO_CONTENT
}
"""

files["projects/taskflow/src/api/middleware.rs"] = """\
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use tower::ServiceBuilder;
use tower_http::limit::RateLimitLayer;

use crate::{auth::validate_token, AppState};

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let Some(token) = token else {
        return (StatusCode::UNAUTHORIZED, "missing Authorization header").into_response();
    };

    match validate_token(token, &state.config.auth.jwt_secret) {
        Ok(claims) => {
            req.extensions_mut().insert(claims);
            next.run(req).await
        }
        Err(_) => (StatusCode::UNAUTHORIZED, "invalid or expired token").into_response(),
    }
}

/// Token-bucket rate limiter keyed by user ID.
/// Limits are read from config at startup; the admin_bypass flag skips
/// rate limiting for admin-role tokens entirely.
pub fn rate_limit_layer(requests_per_minute: u32) -> tower::layer::util::Identity {
    // Placeholder — real implementation uses a per-key sliding window
    // stored in AppState.rate_limiter (DashMap<String, Bucket>).
    tower::layer::util::Identity::new()
}

/// Extracts the user ID from the JWT claims injected by `require_auth`
/// and increments their request counter. Returns 429 if the rate limit
/// is exceeded, with a Retry-After header indicating when the window resets.
pub async fn rate_limit_by_user(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    use crate::auth::Claims;
    let claims = req.extensions().get::<Claims>().cloned();
    if let Some(ref c) = claims {
        if c.role == "admin" && state.config.rate_limit.admin_bypass {
            return next.run(req).await;
        }
        if !state.rate_limiter.check(&c.sub) {
            return (StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded").into_response();
        }
    }
    next.run(req).await
}
"""

files["projects/taskflow/docs/architecture.md"] = """\
# Architecture

## Overview

taskflow is a monolithic REST API with three logical layers:

1. **HTTP layer** (`src/api/`) — routing, authentication middleware, rate limiting
2. **Domain layer** (`src/tasks.rs`, `src/auth.rs`) — business logic
3. **Storage layer** (`src/db.rs`) — SQLite via sqlx, optional in-memory cache

## Authentication

All endpoints (except `POST /auth/login`) require a valid JWT bearer token.

- Tokens are issued as pairs: a short-lived **access token** (1 hour) and a
  long-lived **refresh token** (7 days)
- Signed with HS256 using the `auth.jwt_secret` from config
- Claims include `sub` (user ID), `role`, `iat`, and `exp`
- The `require_auth` middleware validates the token on every request and
  injects `Claims` into request extensions
- Passwords are hashed with bcrypt (cost 12); minimum password length is 12 chars

## Rate Limiting

Rate limiting uses a **sliding window** counter per user, stored in a
`DashMap<UserId, Bucket>` in `AppState`.

- Default: 60 requests/minute with a burst of 10
- Admin accounts can bypass rate limiting (`admin_bypass = true` in config)
- When the limit is exceeded, the API returns `429 Too Many Requests` with a
  `Retry-After` header

## Webhooks

When a task transitions to `done`, the server fires a POST to all registered
webhook URLs for that project:

```json
{
  "event": "task.completed",
  "task_id": 42,
  "project_id": 7,
  "completed_at": "2024-03-15T14:22:00Z"
}
```

Delivery is best-effort with up to 3 retries (exponential backoff).
Webhook secrets are verified with HMAC-SHA256 — the `X-Taskflow-Signature`
header contains `sha256=<hex>` of the request body.

## Caching

Task list queries are cached in memory (LRU, configurable TTL and size).
The cache is invalidated on any write to the relevant project.

## Database Schema

```sql
CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    username      TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'member',
    created_at    TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE projects (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    owner_id   INTEGER REFERENCES users(id),
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE tasks (
    id          INTEGER PRIMARY KEY,
    title       TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'open',
    assignee_id INTEGER REFERENCES users(id),
    project_id  INTEGER NOT NULL REFERENCES projects(id),
    due_date    TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE webhooks (
    id         INTEGER PRIMARY KEY,
    project_id INTEGER NOT NULL REFERENCES projects(id),
    url        TEXT NOT NULL,
    secret     TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```
"""

files["projects/taskflow/docs/deployment.md"] = """\
# Deployment Guide

## Building

```bash
cargo build --release --features webhooks
```

The binary is at `target/release/taskflow`. It has no runtime dependencies
other than the SQLite database file.

## systemd unit

Create `/etc/systemd/system/taskflow.service`:

```ini
[Unit]
Description=taskflow API server
After=network.target

[Service]
Type=simple
User=taskflow
WorkingDirectory=/opt/taskflow
ExecStart=/opt/taskflow/taskflow --config /etc/taskflow/server.toml
Restart=on-failure
RestartSec=5
# Harden the process
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ReadWritePaths=/var/lib/taskflow

[Install]
WantedBy=multi-user.target
```

Enable and start:

```bash
systemctl daemon-reload
systemctl enable --now taskflow
```

## Reverse proxy (nginx)

```nginx
server {
    listen 443 ssl;
    server_name tasks.example.com;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Production config

Always override these in production:

- `auth.jwt_secret` — generate with `openssl rand -hex 32`
- `auth.bcrypt_cost` — increase to 14 for extra security
- `database.path` — use an absolute path outside the working directory
- `rate_limit.requests_per_minute` — tune based on expected traffic
- `cache.enabled = true` — strongly recommended; reduces DB load by ~60%

## Database backups

SQLite can be backed up live using the `.backup` command or by copying the
WAL file. A simple cron entry:

```cron
0 2 * * * sqlite3 /var/lib/taskflow/taskflow.db ".backup '/var/backups/taskflow/$(date +%F).db'"
```

Keep 30 days of backups. The database is small (~1 MB per 10,000 tasks).

## Webhook endpoint requirements

Your webhook receiver must:
- Accept `POST` requests with `Content-Type: application/json`
- Respond with `2xx` within 10 seconds
- Verify the `X-Taskflow-Signature` header to authenticate payloads
- Be accessible from the server running taskflow (check firewall rules)

## Monitoring

taskflow emits structured JSON logs. Feed them to your log aggregator.
Key fields: `level`, `message`, `user_id`, `endpoint`, `duration_ms`,
`status_code`.

Recommended alerts:
- `status_code=5xx` rate > 1%
- `duration_ms` p99 > 500
- Authentication failure rate > 10/min (possible credential stuffing)
- Rate limit hit rate > 5% (may need to raise limits or investigate abuse)
"""

# ── weather-cli: fictional Python weather CLI ─────────────────────────────────

files["projects/weather-cli/README.md"] = """\
# weather-cli

A command-line weather tool that fetches forecasts from the Open-Meteo API
and caches results locally to avoid hammering the endpoint.

## Install

```bash
pip install -e .
# or
pipx install .
```

## Usage

```bash
weather london
weather --days 5 "New York"
weather --json tokyo | jq '.daily.temperature_max'
```

## Configuration

Create `~/.config/weather-cli/config.toml`:

```toml
[api]
base_url = "https://api.open-meteo.com/v1"
timeout_secs = 10

[cache]
enabled = true
ttl_secs = 1800
directory = "~/.cache/weather-cli"

[display]
units = "celsius"   # or "fahrenheit"
wind_speed = "kmh"
```

## How it works

1. Geocode the location name via the Open-Meteo geocoding API
2. Check the local cache (`~/.cache/weather-cli/`) for a recent response
3. If the cache is stale or missing, fetch a fresh forecast
4. Render the output as a Unicode table or raw JSON (`--json`)
"""

files["projects/weather-cli/pyproject.toml"] = """\
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "weather-cli"
version = "0.3.1"
description = "Command-line weather forecasts with local caching"
requires-python = ">=3.11"
dependencies = [
    "httpx>=0.27",
    "rich>=13",
    "tomllib; python_version < '3.11'",
    "platformdirs>=4",
]

[project.scripts]
weather = "weather_cli.main:main"

[tool.hatch.envs.dev]
dependencies = ["pytest", "pytest-httpx", "ruff", "mypy"]
"""

files["projects/weather-cli/src/main.py"] = """\
#!/usr/bin/env python3
\"\"\"weather-cli: command-line weather forecasts.\"\"\"

import argparse
import json
import sys

from weather_cli.api import WeatherClient, GeocodingError, ApiError
from weather_cli.cache import Cache
from weather_cli.display import render_forecast

DEFAULT_DAYS = 3


def main() -> None:
    parser = argparse.ArgumentParser(
        prog="weather",
        description="Fetch weather forecasts from the command line.",
    )
    parser.add_argument("location", help="City name or 'lat,lon'")
    parser.add_argument("--days", type=int, default=DEFAULT_DAYS,
                        help=f"Number of forecast days (default: {DEFAULT_DAYS})")
    parser.add_argument("--json", action="store_true", dest="json_output",
                        help="Output raw JSON instead of formatted table")
    parser.add_argument("--no-cache", action="store_true",
                        help="Bypass cache and fetch a fresh forecast")
    parser.add_argument("--units", choices=["celsius", "fahrenheit"],
                        default="celsius")
    args = parser.parse_args()

    cache = Cache()
    client = WeatherClient()

    try:
        if not args.no_cache:
            cached = cache.get(args.location, args.days)
            if cached:
                forecast = cached
            else:
                forecast = client.fetch(args.location, args.days)
                cache.put(args.location, args.days, forecast)
        else:
            forecast = client.fetch(args.location, args.days)

    except GeocodingError as e:
        print(f"error: could not find location '{args.location}': {e}", file=sys.stderr)
        sys.exit(1)
    except ApiError as e:
        print(f"error: API request failed: {e}", file=sys.stderr)
        sys.exit(1)

    if args.json_output:
        print(json.dumps(forecast, indent=2))
    else:
        render_forecast(forecast, units=args.units)
"""

files["projects/weather-cli/src/api.py"] = """\
\"\"\"HTTP client for the Open-Meteo weather and geocoding APIs.\"\"\"

import hashlib
import json
import time
from pathlib import Path
from typing import Any

import httpx

GEOCODING_URL = "https://geocoding-api.open-meteo.com/v1/search"
FORECAST_URL  = "https://api.open-meteo.com/v1/forecast"
TIMEOUT_SECS  = 10

# Cache responses for 30 minutes by default to avoid rate-limit issues.
CACHE_TTL_SECS = 1800
CACHE_DIR = Path.home() / ".cache" / "weather-cli"


class GeocodingError(Exception):
    pass


class ApiError(Exception):
    pass


class WeatherClient:
    def __init__(self, timeout: int = TIMEOUT_SECS):
        self.timeout = timeout
        self._http = httpx.Client(timeout=timeout)

    def geocode(self, location: str) -> tuple[float, float, str]:
        \"\"\"Return (latitude, longitude, display_name) for a location string.\"\"\"
        resp = self._http.get(GEOCODING_URL, params={"name": location, "count": 1})
        resp.raise_for_status()
        data = resp.json()
        results = data.get("results")
        if not results:
            raise GeocodingError(f"no results for '{location}'")
        r = results[0]
        return r["latitude"], r["longitude"], r.get("name", location)

    def fetch(self, location: str, days: int = 3) -> dict[str, Any]:
        \"\"\"Fetch a weather forecast for `location` covering `days` days.\"\"\"
        lat, lon, name = self.geocode(location)
        params = {
            "latitude": lat,
            "longitude": lon,
            "daily": [
                "temperature_2m_max",
                "temperature_2m_min",
                "precipitation_sum",
                "windspeed_10m_max",
                "weathercode",
            ],
            "forecast_days": days,
            "timezone": "auto",
        }
        try:
            resp = self._http.get(FORECAST_URL, params=params)
            resp.raise_for_status()
        except httpx.HTTPStatusError as e:
            raise ApiError(f"HTTP {e.response.status_code}: {e.response.text}") from e
        except httpx.RequestError as e:
            raise ApiError(f"request failed: {e}") from e

        data = resp.json()
        data["_location_name"] = name
        data["_fetched_at"] = time.time()
        return data

    def __enter__(self):
        return self

    def __exit__(self, *args):
        self._http.close()
"""

# ── notes ─────────────────────────────────────────────────────────────────────

files["notes/meeting-notes.md"] = """\
# Meeting Notes

## 2024-03-14 — Sprint planning

**Attendees:** Alice, Ben, Carol, Dan

### Taskflow v0.5 priorities

1. **Webhook retry logic** — currently retries are synchronous; need to move to
   a background queue so failed deliveries don't block the API response. Ben
   will look at using a simple SQLite-backed queue.

2. **Rate limit dashboard** — product wants a `/admin/rate-limits` endpoint
   showing per-user request counts and reset times. Carol has a draft in
   the `rate-limit-stats` branch.

3. **Password reset flow** — the forgot-password email endpoint is blocked on
   the SMTP service integration. Dan to unblock by end of sprint.

4. **Cache invalidation bug** — tasks don't always refresh after an update when
   cache TTL is short. Likely a race condition in `cache.rs`. Alice is
   investigating.

### Authentication improvements (v0.6 candidate)

- Consider switching from HS256 to RS256 JWT signing so we can rotate keys
  without redeploying. Deferred to next sprint.
- WebAuthn/passkey support mentioned but not yet scoped.

### Infrastructure

- Deploy to staging using the new systemd unit file from the deployment guide.
- Set `auth.bcrypt_cost = 14` in staging to match production hardening.
- Ensure webhook receiver firewall rules are in place before demo.

---

## 2024-03-07 — Architecture review

**Topic:** Caching strategy for task list queries

Current approach: whole-response cache keyed on `(project_id, user_id)`.
Problem: fine-grained invalidation is hard — any task write invalidates the
entire project cache.

**Decision:** Move to a row-level cache (`task_id → Task`). Task list queries
assemble results from the row cache and fall back to DB for misses. This
reduces cache churn on busy projects.

Dan will prototype the change. Target: 30% reduction in DB queries at p50.

---

## 2024-02-22 — Security review

Reviewed authentication and rate limiting implementation with security team.

**Findings:**
- JWT secret rotation needs a documented procedure — currently requires restart
- Rate limits should be per-IP in addition to per-user for unauthenticated paths
- Password minimum length of 12 is acceptable; recommend adding a complexity
  check (uppercase + digit + symbol)
- Webhook HMAC verification is correctly implemented
- No SQL injection vectors found in sqlx queries (using query macros)

**Action items:**
- Document secret rotation runbook in `docs/deployment.md`
- Add per-IP rate limiting to the login endpoint to mitigate credential stuffing
- Add password complexity validation to `auth.rs`
"""

files["notes/research-databases.md"] = """\
# Database Research Notes

## SQLite vs PostgreSQL for taskflow

### Current: SQLite

Pros:
- Zero-deployment: no separate process to manage
- WAL mode gives good read concurrency
- sqlx supports it natively
- The database is ~1 MB per 10,000 tasks — easily fits in memory
- Single-file backups are trivial

Cons:
- Write throughput limited (~1,000 writes/sec with WAL)
- No connection pooling across processes
- No native full-text search as capable as PostgreSQL's `tsvector`

### Alternative: PostgreSQL

Pros:
- Better write throughput and concurrency
- Native full-text search (`tsvector`, `GIN` indexes)
- Row-level locking, advisory locks
- Connection pooling with PgBouncer

Cons:
- Requires a separate process and more ops overhead
- More complex deployment (authentication, backups, replication)

### Decision

Stick with SQLite for v1. The rate limiting middleware handles peak write
bursts adequately, and task search can use `LIKE` until full-text is needed.
Revisit if write throughput exceeds 500 req/s sustained.

---

## Cache backends considered

| Backend | TTL support | Persistence | Complexity |
|---------|-------------|-------------|------------|
| In-process HashMap | Manual | No | Low |
| `moka` (LRU crate) | Yes | No | Low |
| Redis | Yes | Optional | Medium |
| Memcached | Yes | No | Medium |

**Chosen:** `moka` — provides LRU eviction, per-entry TTL, and async support
with no external dependencies. Cache entries survive server restarts only if
SQLite results are fresh (TTL check on load).

---

## Full-text search options

Currently using SQL `LIKE` queries for task search. Options to improve:

1. **SQLite FTS5** — available in sqlx, requires a shadow table, decent for
   small datasets
2. **PostgreSQL tsvector** — best quality, requires migration
3. **Tantivy** — Rust-native, can be embedded, fast, but adds binary size
4. **Meilisearch / Typesense** — external service, great UX, ops overhead

For now: SQLite FTS5 when we need better search. External service if we
ever offer multi-tenant SaaS.
"""

files["notes/ideas.md"] = """\
# Ideas & Backlog

## Taskflow feature ideas

### High priority

- **Bulk task operations** — mark multiple tasks done/archived in one request.
  Useful for sprint close-out. API: `POST /projects/:id/tasks/bulk` with a list
  of IDs and an action.

- **Task dependencies** — `blocks` / `blocked_by` relationships. Needs schema
  change (new `task_edges` table). Visualise as a dependency graph in the UI.

- **Saved searches** — let users save a search query and subscribe to updates.
  Could be implemented as a special webhook trigger.

### Medium priority

- **Rate limit tiers** — different limits for free vs paid accounts. The current
  flat `requests_per_minute` config isn't granular enough. Consider per-role
  limits: `admin=unlimited`, `member=120/min`, `viewer=30/min`.

- **Audit log** — append-only table recording all authentication events (login,
  logout, password change, token refresh) and task mutations. Important for
  compliance.

- **Comment threads on tasks** — each task gets a timeline of comments.
  Markdown-formatted, with `@mention` support.

### Low priority / speculative

- WebSocket support for real-time task updates (instead of webhook polling)
- CLI client (`taskflow-cli`) using the REST API
- GitHub/GitLab issue sync via webhook
- Time tracking (start/stop timer per task)
- Dark mode for the web UI

## weather-cli ideas

- Add a `--hourly` flag for hourly forecasts
- Cache to SQLite instead of flat JSON files — easier TTL management
- Support for historical weather data (Open-Meteo historical API)
- Shell completions (fish, zsh, bash)
- `--alert` flag to print warnings when rain/wind thresholds are exceeded
- Configurable display: emoji icons for weather codes, colour themes

## Tooling / DX ideas

- Pre-commit hook to run `cargo clippy` and `ruff check` before every commit
- GitHub Actions CI: test + clippy on push, build release binary on tag
- Dependabot for Cargo.toml and pyproject.toml
- `docker-compose.yml` for local development (taskflow + a test webhook receiver)
"""

files["notes/onboarding-checklist.md"] = """\
# New Developer Onboarding Checklist

Welcome to the team! Work through this list in order.

## Day 1: Access & tools

- [ ] Get added to GitHub org and relevant repos
- [ ] Clone `taskflow` and `weather-cli` repos
- [ ] Install Rust toolchain (`rustup`) and cargo tools:
  - `cargo install cargo-watch cargo-nextest`
- [ ] Install Python 3.11+ and set up a virtual environment for `weather-cli`
- [ ] Run both projects locally (see each README)
- [ ] Get credentials for the staging environment

## Day 2: Codebase orientation

### taskflow

- [ ] Read `docs/architecture.md` — understand the authentication flow,
      rate limiting, and webhook delivery pipeline
- [ ] Read `docs/deployment.md` — understand the systemd unit and nginx config
- [ ] Browse `src/auth.rs` — JWT issuance, password hashing with bcrypt
- [ ] Browse `src/api/middleware.rs` — how authentication and rate limiting
      are applied per-request
- [ ] Run the test suite: `cargo nextest run`

### weather-cli

- [ ] Read the README — understand the cache layer and API client structure
- [ ] Browse `src/api.py` — geocoding + forecast fetch with caching
- [ ] Run tests: `pytest`

## Week 1: First contribution

- [ ] Pick a "good first issue" from the backlog (tagged in GitHub)
- [ ] Set up branch protection understanding (don't push to `main` directly)
- [ ] Submit your first PR — even a small documentation fix counts
- [ ] Pair with a team member on a code review

## Security reminders

- Never commit secrets (API keys, JWT secrets, passwords) to the repo
- Use `git secrets` or similar to prevent accidental credential commits
- The `auth.jwt_secret` in `config/default.toml` is for local dev only;
  production secrets are managed separately
- Report any suspected security issues privately before opening a public issue
- All authentication events are logged — if you see unexpected login attempts
  in the audit log, escalate immediately
"""

# ── JSON files ────────────────────────────────────────────────────────────────

json_files = {}

json_files["projects/taskflow/tests/fixtures/create_task.json"] = """\
{
  "title": "Implement webhook retry queue",
  "description": "Move webhook delivery to a background queue backed by SQLite so that failed deliveries don't block the API response. Retry up to 3 times with exponential backoff.",
  "assignee_id": 4,
  "due_date": "2024-03-28"
}
"""

json_files["projects/taskflow/tests/fixtures/task_response.json"] = """\
{
  "id": 142,
  "title": "Implement webhook retry queue",
  "description": "Move webhook delivery to a background queue backed by SQLite so that failed deliveries don't block the API response. Retry up to 3 times with exponential backoff.",
  "status": "open",
  "assignee_id": 4,
  "project_id": 7,
  "due_date": "2024-03-28",
  "created_at": "2024-03-15T10:42:00Z",
  "updated_at": "2024-03-15T10:42:00Z"
}
"""

json_files["projects/taskflow/tests/fixtures/auth_token_response.json"] = """\
{
  "access_token": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI3IiwicGlsZSI6Im1lbWJlciIsImV4cCI6MTcxMDUwMDAwMH0.placeholder",
  "refresh_token": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiI3Iiwicm9sZSI6Im1lbWJlciIsImV4cCI6MTcxMTEwNDgwMH0.placeholder",
  "expires_in": 3600
}
"""

json_files["projects/taskflow/tests/fixtures/rate_limit_error.json"] = """\
{
  "error": "rate_limit_exceeded",
  "message": "Too many requests. You have exceeded the limit of 60 requests per minute.",
  "retry_after": 23
}
"""

json_files["projects/weather-cli/tests/fixtures/forecast_london.json"] = """\
{
  "latitude": 51.5085,
  "longitude": -0.1257,
  "timezone": "Europe/London",
  "timezone_abbreviation": "GMT",
  "_location_name": "London",
  "_fetched_at": 1710500000.0,
  "daily_units": {
    "time": "iso8601",
    "temperature_2m_max": "°C",
    "temperature_2m_min": "°C",
    "precipitation_sum": "mm",
    "windspeed_10m_max": "km/h",
    "weathercode": "wmo code"
  },
  "daily": {
    "time": ["2024-03-15", "2024-03-16", "2024-03-17"],
    "temperature_2m_max": [12.4, 10.1, 8.7],
    "temperature_2m_min": [5.2, 4.8, 3.1],
    "precipitation_sum": [0.0, 3.2, 7.8],
    "windspeed_10m_max": [18.5, 24.2, 31.0],
    "weathercode": [1, 61, 65]
  }
}
"""

json_files["notes/contacts.json"] = """\
[
  {
    "id": 1,
    "name": "Alice Nakamura",
    "role": "Backend Engineer",
    "email": "alice@example.com",
    "github": "alicen",
    "timezone": "America/New_York",
    "notes": "Taskflow lead. Owns the cache layer and DB schema."
  },
  {
    "id": 2,
    "name": "Ben Okafor",
    "role": "Backend Engineer",
    "email": "ben@example.com",
    "github": "benokafor",
    "timezone": "Europe/London",
    "notes": "Working on webhook retry queue. Good on infrastructure and deployment."
  },
  {
    "id": 3,
    "name": "Carol Diaz",
    "role": "Full-stack Engineer",
    "email": "carol@example.com",
    "github": "caroldiaz",
    "timezone": "Europe/Madrid",
    "notes": "Rate limiting dashboard, frontend work."
  },
  {
    "id": 4,
    "name": "Dan Reeves",
    "role": "DevOps / SRE",
    "email": "dan@example.com",
    "github": "danreeves",
    "timezone": "America/Chicago",
    "notes": "Authentication improvements, SMTP integration, production deployment."
  }
]
"""

# ── JPEG images with EXIF metadata ────────────────────────────────────────────
# Each entry: (relative_path, width, height, color_rgb, exif_dict, description)

def make_jpeg(width, height, color, exif_bytes):
    """Create a minimal solid-colour JPEG with the given EXIF bytes."""
    from PIL import Image
    img = Image.new("RGB", (width, height), color)
    buf = io.BytesIO()
    img.save(buf, format="JPEG", exif=exif_bytes, quality=85)
    return buf.getvalue()


def gps_dms(decimal_degrees):
    """Convert decimal degrees to ((d,1),(m,1),(s*100,100)) EXIF rational tuple."""
    d = int(abs(decimal_degrees))
    m = int((abs(decimal_degrees) - d) * 60)
    s = round(((abs(decimal_degrees) - d) * 60 - m) * 6000)
    return ((d, 1), (m, 1), (s, 100))


image_specs = [
    # Team offsite photo — San Francisco, shot on a Fujifilm X-T5
    (
        "notes/photos/team-offsite-2024.jpg",
        1280, 853,
        (110, 130, 95),   # outdoor greenish
        {
            "0th": {
                271: b"FUJIFILM\x00",          # Make
                272: b"X-T5\x00",              # Model
                274: 1,                         # Orientation: normal
                305: b"Capture One 23\x00",    # Software
                306: b"2024:03:14 13:22:45\x00", # DateTime
                315: b"Carol Diaz\x00",         # Artist
                270: b"Team offsite, Golden Gate Park, San Francisco\x00",  # ImageDescription
            },
            "Exif": {
                36867: b"2024:03:14 13:22:45\x00",  # DateTimeOriginal
                36868: b"2024:03:14 13:22:45\x00",  # DateTimeDigitized
                33437: (1, 8),           # FNumber f/8
                33434: (1, 500),         # ExposureTime 1/500s
                34855: 320,              # ISOSpeedRatings
                37386: (35, 1),          # FocalLength 35mm
                41486: (35, 1),          # FocalLengthIn35mmFilm
                37385: 0,                # Flash: no flash
                41990: 0,                # WhiteBalance: auto
            },
            "GPS": {
                1:  b"N",                              # GPSLatitudeRef
                2:  gps_dms(37.7694),                  # GPSLatitude  (37°N → GG Park)
                3:  b"W",                              # GPSLongitudeRef
                4:  gps_dms(122.4862),                 # GPSLongitude
                5:  0,                                 # GPSAltitudeRef: above sea level
                6:  (52, 1),                           # GPSAltitude 52m
                12: b"K\x00",                          # GPSSpeedRef: km/h
                13: (0, 1),                            # GPSSpeed: stationary
            },
        },
    ),
    # Whiteboard photo from architecture review — London office, shot on a Pixel 8
    (
        "notes/photos/architecture-whiteboard.jpg",
        1024, 768,
        (230, 225, 210),  # whiteboard-ish off-white
        {
            "0th": {
                271: b"Google\x00",
                272: b"Pixel 8\x00",
                274: 6,    # Orientation: rotated 90° CW (phone held portrait)
                305: b"Android 14\x00",
                306: b"2024:03:07 11:08:03\x00",
                315: b"Ben Okafor\x00",
                270: b"Architecture review whiteboard - caching strategy\x00",
            },
            "Exif": {
                36867: b"2024:03:07 11:08:03\x00",
                36868: b"2024:03:07 11:08:03\x00",
                33437: (1, 2),    # f/1.8
                33434: (1, 60),   # 1/60s (indoor)
                34855: 800,
                37386: (6, 1),    # 6mm (wide on a phone)
                37385: 0,         # no flash
                41990: 0,
            },
            "GPS": {
                1:  b"N",
                2:  gps_dms(51.5074),   # London
                3:  b"W",
                4:  gps_dms(0.1278),
                5:  0,
                6:  (15, 1),
            },
        },
    ),
    # Product screenshot export — no GPS, just camera metadata
    (
        "projects/taskflow/docs/screenshots/task-list.jpg",
        1440, 900,
        (13, 17, 23),     # dark UI background colour
        {
            "0th": {
                271: b"Apple\x00",
                272: b"MacBook Pro\x00",
                305: b"macOS Sequoia 15.2\x00",
                306: b"2024:03:20 16:45:00\x00",
                270: b"Taskflow task list - sprint view\x00",
            },
            "Exif": {
                36867: b"2024:03:20 16:45:00\x00",
                36868: b"2024:03:20 16:45:00\x00",
            },
        },
    ),
]

# ── Write all files ───────────────────────────────────────────────────────────

# ── Archives ──────────────────────────────────────────────────────────────────

def make_archives(base: str) -> int:
    import zipfile
    import tarfile
    count = 0

    # ── 1. taskflow-v0.4.2-docs.zip ──────────────────────────────────────────
    # A "release docs" zip — what a user might download separately from the binary.
    zip_path = os.path.join(base, "projects/taskflow/releases/taskflow-v0.4.2-docs.zip")
    os.makedirs(os.path.dirname(zip_path), exist_ok=True)
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("taskflow-v0.4.2/README.md", """\
# taskflow v0.4.2 — Release Notes

## What's new

- **Webhook retry queue**: failed webhook deliveries are now retried up to 3
  times in the background with exponential backoff, so delivery failures no
  longer block the API response.
- **Rate limit stats endpoint**: `GET /admin/rate-limits` returns per-user
  request counts and window reset times.
- **Password complexity validation**: `POST /auth/register` now enforces a
  minimum password length of 12 characters with at least one uppercase letter,
  one digit, and one symbol.

## Bug fixes

- Fixed a race condition in the cache layer that caused stale task lists to be
  returned for up to 30 seconds after an update.
- The `deploy` subcommand now correctly reads `auth.jwt_secret` from environment
  variables, allowing secrets to be injected at runtime without touching the
  config file.

## Upgrading from v0.4.1

No schema changes. Restart the service after deploying the new binary.
If you use the webhook feature, enable the `webhooks` Cargo feature flag:

```bash
cargo build --release --features webhooks
```

## Known issues

- The `GET /admin/rate-limits` endpoint requires admin authentication but does
  not yet support pagination. Will be addressed in v0.4.3.
""")
        zf.writestr("taskflow-v0.4.2/CHANGELOG.md", """\
# Changelog

## [0.4.2] — 2024-03-15

### Added
- Webhook retry queue with SQLite-backed job store
- `GET /admin/rate-limits` — rate limit statistics per user
- Password complexity validation (min 12 chars, uppercase, digit, symbol)
- `Retry-After` header on 429 responses

### Fixed
- Cache race condition causing stale task list responses
- `deploy` subcommand not reading JWT secret from environment
- Rate limiter not resetting correctly after window expiry

## [0.4.1] — 2024-02-28

### Added
- Role-based access control: admin, member, viewer
- `POST /auth/refresh` endpoint for token refresh
- Per-user rate limiting with admin bypass option

### Fixed
- JWT validation rejecting tokens 1 second before expiry due to clock skew
- bcrypt cost not being read from config (was always using DEFAULT_COST=12)

## [0.4.0] — 2024-02-01

### Added
- Initial authentication system (JWT, bcrypt password hashing)
- Task CRUD endpoints
- Webhook notifications on task completion
- In-memory cache for task list queries
""")
        zf.writestr("taskflow-v0.4.2/docs/api-reference.md", """\
# API Reference

Base URL: `https://your-host/`

All endpoints except `POST /auth/login` require:
```
Authorization: Bearer <access_token>
```

---

## Authentication

### POST /auth/login

Request:
```json
{ "username": "alice", "password": "correct-horse-battery-staple" }
```

Response `200 OK`:
```json
{
  "access_token": "eyJ...",
  "refresh_token": "eyJ...",
  "expires_in": 3600
}
```

Errors:
- `401 Unauthorized` — invalid credentials
- `429 Too Many Requests` — rate limit exceeded (includes `Retry-After` header)

### POST /auth/refresh

Exchange a refresh token for a new access token. Refresh tokens are valid
for 7 days. Rotate the refresh token on each use.

---

## Tasks

### GET /projects/:id/tasks

Returns all tasks for a project. Supports `?q=search+term` for full-text
search across title and description.

### POST /projects/:id/tasks

Create a task. Body: `CreateTask` JSON.

### POST /tasks/:id/complete

Mark a task as done. Fires a webhook to all registered receivers for the
project with event `task.completed`.

---

## Webhooks

### POST /webhooks

Register a webhook URL for a project. Provide a `secret` for HMAC-SHA256
signature verification. Deliveries include an `X-Taskflow-Signature` header.

### DELETE /webhooks/:id

Remove a webhook registration.

---

## Admin

### GET /admin/rate-limits

Returns per-user rate limit statistics. Requires `admin` role.

### POST /admin/rate-limits/:user_id/reset

Reset the rate limit window for a specific user. Useful after an abuse
incident or when onboarding a high-traffic integration.
""")
        zf.writestr("taskflow-v0.4.2/config/production.toml.example", """\
[server]
host = "127.0.0.1"
port = 8080

[database]
path = "/var/lib/taskflow/taskflow.db"
max_connections = 10

[auth]
# Generate with: openssl rand -hex 32
jwt_secret = "REPLACE_WITH_LONG_RANDOM_SECRET"
token_expiry_secs = 3600
refresh_expiry_secs = 604800
bcrypt_cost = 14
password_min_length = 12

[rate_limit]
enabled = true
requests_per_minute = 120
burst = 20
admin_bypass = true

[cache]
enabled = true
ttl_secs = 30
max_entries = 5000

[webhooks]
enabled = true
timeout_secs = 10
retry_attempts = 3

[logging]
level = "info"
format = "json"
""")
    sz = os.path.getsize(zip_path)
    print(f"  wrote projects/taskflow/releases/taskflow-v0.4.2-docs.zip  ({sz//1024}KB, 4 members)")
    count += 1

    # ── 2. notes/archive/2023-notes-backup.tar.gz ────────────────────────────
    # A "backup" tar of older meeting notes.
    tar_path = os.path.join(base, "notes/archive/2023-notes-backup.tar.gz")
    os.makedirs(os.path.dirname(tar_path), exist_ok=True)
    with tarfile.open(tar_path, "w:gz") as tf:
        def add_str(name, content):
            data = content.encode()
            info = tarfile.TarInfo(name=name)
            info.size = len(data)
            info.mtime = 1704067200  # 2024-01-01
            tf.addfile(info, io.BytesIO(data))

        add_str("2023-notes/q4-planning.md", """\
# Q4 2023 Planning Notes

## Goals

- Ship authentication system (JWT + bcrypt) — target: end of October
- Webhook notifications MVP — target: mid November
- Rate limiting — target: end of November
- Deploy to production — target: December 1

## Risks

- bcrypt cost tuning: too low = security risk, too high = slow login.
  Benchmark on target hardware before deploying.
- Webhook delivery reliability: need retry logic before going live.
  Don't want failed webhooks silently dropped.

## Authentication design decisions

Chose HS256 JWT over RS256 for simplicity. Can migrate to RS256 later
if we need key rotation without service restart. Refresh tokens stored
server-side in SQLite to support revocation.

Password policy: minimum 12 characters. Will add complexity rules in
a follow-up. Considered passkeys but deferred to 2024.
""")
        add_str("2023-notes/q3-retrospective.md", """\
# Q3 2023 Retrospective

## What went well

- Database schema is clean and well-normalised
- SQLite choice working out — WAL mode handles our write load fine
- Cache implementation reduced DB query load by ~40% in load tests

## What could be better

- Need better observability — currently only have application logs,
  no metrics dashboard. Add Prometheus endpoint in Q4.
- Deploy process is manual — need to automate with systemd and a
  deployment script. Currently copy-pasting commands from the docs.
- Rate limiting was deprioritised — now a Q4 blocker since we're
  opening the API to external integrations.

## Action items

- Write deployment automation script (Dan)
- Add /metrics endpoint with request counts, latency histograms (Alice)
- Document rate limit design and get sign-off before implementation (Carol)
""")
        add_str("2023-notes/security-checklist.md", """\
# Security Checklist (Pre-launch)

## Authentication
- [x] Passwords hashed with bcrypt (cost 12, increase to 14 for prod)
- [x] JWT signed with HS256; secret stored in config, not hardcoded
- [x] Refresh tokens stored server-side (revocable)
- [x] Token expiry enforced on every request
- [ ] Rate limiting on /auth/login (prevent credential stuffing)
- [ ] Account lockout after N failed attempts

## Transport
- [x] TLS termination at nginx (redirect HTTP → HTTPS)
- [x] HSTS header set
- [ ] Certificate auto-renewal configured (Let's Encrypt / certbot)

## API
- [x] All endpoints require authentication (except /auth/login)
- [x] sqlx query macros used throughout (no SQL injection vectors)
- [x] Webhook payloads signed with HMAC-SHA256
- [ ] Input validation on all user-supplied fields (title length, etc.)

## Infrastructure
- [x] Service runs as unprivileged user (taskflow)
- [x] systemd hardening: NoNewPrivileges, PrivateTmp, ProtectSystem
- [ ] Database backups automated and tested
- [ ] Log aggregation configured
""")

    sz = os.path.getsize(tar_path)
    print(f"  wrote notes/archive/2023-notes-backup.tar.gz  ({sz//1024}KB, 3 members)")
    count += 1

    return count


def write_files(base: str, files: dict) -> None:
    for rel, content in files.items():
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(content)
        print(f"  wrote {rel}")


def write_binary_files(base: str) -> int:
    import piexif
    count = 0
    for rel, width, height, color, exif_dict, *_ in image_specs:
        path = os.path.join(base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        exif_bytes = piexif.dump(exif_dict)
        data = make_jpeg(width, height, color, exif_bytes)
        with open(path, "wb") as f:
            f.write(data)
        print(f"  wrote {rel}  ({width}×{height}, {len(data)//1024}KB)")
        count += 1
    return count


if __name__ == "__main__":
    import shutil
    if os.path.exists(ROOT):
        shutil.rmtree(ROOT)
    print(f"Generating demo data in {ROOT}/")
    write_files(ROOT, files)
    write_files(ROOT, json_files)
    n_images = write_binary_files(ROOT)
    n_archives = make_archives(ROOT)
    total = len(files) + len(json_files) + n_images + n_archives
    print(f"\nDone — {total} files created ({len(files)} text, {len(json_files)} JSON, {n_images} JPEG, {n_archives} archives).")
    print("\nAdd to find-client config:")
    print('  [[sources]]')
    print('  name  = "projects"')
    print(f'  paths = ["{ROOT}/projects"]')
    print()
    print('  [[sources]]')
    print('  name  = "notes"')
    print(f'  paths = ["{ROOT}/notes"]')
