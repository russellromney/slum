# Slum Implementation Plan: Dashboard, Logging, Metrics & Tenement Merge

## Executive Summary

This plan transforms slum from a fleet orchestrator into a unified platform combining:
- **Fleet management** (slum) - routing requests to tenant servers
- **Process supervision** (tenement) - managing app instances on each server
- **Dashboard** - Svelte SPA for sysadmin tasks
- **Logging** - SQLite + FTS5 with SSE streaming
- **Metrics** - in-memory ring buffers + SQLite history

Single binary deployment works for both single-server and multi-server scenarios.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    Slum Server (port 80/8080)               │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  ┌─────────────────┐  ┌─────────────────┐                  │
│  │ Dashboard (/_/) │  │ Fleet API       │                  │
│  │ - Overview      │  │ /api/servers    │                  │
│  │ - Servers       │  │ /api/tenants    │                  │
│  │ - Tenants       │  │ /api/health     │                  │
│  │ - Instances     │  └─────────────────┘                  │
│  │ - Logs          │                                       │
│  └─────────────────┘  ┌─────────────────┐                  │
│                       │ Proxy (fallback)│                  │
│  Auth: Bearer Token   │ subdomain→tenant│                  │
│                       └─────────────────┘                  │
├─────────────────────────────────────────────────────────────┤
│                    Hypervisor (tenement)                    │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐                    │
│  │ app:user1│ │ app:user2│ │ api:main │  ...               │
│  │ :3001    │ │ :3002    │ │ :4000    │                    │
│  └──────────┘ └──────────┘ └──────────┘                    │
├─────────────────────────────────────────────────────────────┤
│                    SQLite (slum.db)                         │
│  servers | tenants | domain_aliases | logs | metrics | cfg │
└─────────────────────────────────────────────────────────────┘
```

---

## Phase 1: Merge Tenement into Slum (Foundation)

### 1.1 New Directory Structure

```
slum/
├── Cargo.toml              # Workspace root
├── Cargo.lock
├── Makefile
├── README.md
├── pyproject.toml
├── IMPLEMENTATION_PLAN.md  # This file
│
├── slum/                   # Main crate (current src/ renamed)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs         # Unified CLI
│       ├── lib.rs          # Library exports
│       ├── api.rs          # Fleet management API
│       ├── db.rs           # Database layer
│       ├── proxy.rs        # Reverse proxy
│       ├── python.rs       # PyO3 bindings
│       ├── auth.rs         # NEW: Bearer token auth
│       ├── dashboard.rs    # NEW: Dashboard routes + embedded assets
│       ├── logs.rs         # NEW: Log storage + SSE streaming
│       ├── metrics.rs      # NEW: Metrics collection
│       └── sse.rs          # NEW: SSE utilities
│
├── tenement/               # Moved from separate repo
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── config.rs       # TOML config parsing
│       ├── hypervisor.rs   # Process management
│       └── instance.rs     # Instance types
│
└── dashboard/              # NEW: Svelte SPA
    ├── package.json
    ├── svelte.config.js
    ├── vite.config.js
    ├── tailwind.config.js
    ├── tsconfig.json
    ├── src/
    │   ├── app.html
    │   ├── app.css
    │   ├── lib/
    │   │   ├── api.ts
    │   │   ├── stores.ts
    │   │   └── types.ts
    │   └── routes/
    │       ├── +layout.svelte
    │       ├── +page.svelte           # Overview
    │       ├── servers/+page.svelte
    │       ├── tenants/+page.svelte
    │       ├── tenants/[id]/+page.svelte
    │       ├── instances/+page.svelte
    │       └── logs/+page.svelte
    └── static/
```

### 1.2 Workspace Cargo.toml (Root)

```toml
[workspace]
resolver = "2"
members = ["slum", "tenement"]

[workspace.package]
version = "0.2.0"
edition = "2021"
license = "Apache-2.0"
repository = "https://github.com/russellromney/slum"

[workspace.dependencies]
# Core
tokio = { version = "1", features = ["full", "process"] }
axum = { version = "0.7", features = ["macros"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["trace", "cors", "compression-gzip"] }

# Database
sqlx = { version = "0.7", features = ["runtime-tokio", "sqlite"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"

# HTTP Client
hyper = { version = "1", features = ["client", "http1"] }
hyper-util = { version = "0.1", features = ["tokio", "client-legacy"] }
http-body-util = "0.1"

# CLI
clap = { version = "4", features = ["derive"] }

# Error handling
anyhow = "1"
thiserror = "1"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Utilities
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4"] }
```

### 1.3 Slum Crate Cargo.toml

```toml
[package]
name = "slum"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
description = "Fleet orchestrator with process supervision and dashboard"

[[bin]]
name = "slum"
path = "src/main.rs"

[lib]
name = "slum"
crate-type = ["cdylib", "rlib"]

[features]
default = []
python = ["pyo3"]

[dependencies]
# Internal
tenement = { path = "../tenement" }

# Workspace deps
tokio.workspace = true
axum.workspace = true
tower.workspace = true
tower-http.workspace = true
sqlx.workspace = true
serde.workspace = true
serde_json.workspace = true
hyper.workspace = true
hyper-util.workspace = true
http-body-util.workspace = true
clap.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true
tracing-subscriber.workspace = true
chrono.workspace = true
uuid.workspace = true

# New dependencies
rust-embed = { version = "8", features = ["compression"] }
mime_guess = "2"
argon2 = "0.5"
rand = "0.8"
axum-extra = { version = "0.9", features = ["typed-header"] }
tokio-stream = "0.1"
async-broadcast = "0.7"

# Optional Python bindings
pyo3 = { version = "0.22", features = ["extension-module"], optional = true }

[dev-dependencies]
tempfile = "3"
```

### 1.4 Tenement Crate Cargo.toml

```toml
[package]
name = "tenement"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "Process hypervisor for single-server deployments"

[lib]
name = "tenement"
path = "src/lib.rs"

[dependencies]
tokio.workspace = true
serde.workspace = true
serde_json.workspace = true
toml.workspace = true
anyhow.workspace = true
thiserror.workspace = true
tracing.workspace = true

[dev-dependencies]
tempfile = "3"
```

### 1.5 Unified CLI Commands

```rust
// slum/src/main.rs

#[derive(Parser)]
#[command(name = "slum")]
#[command(about = "Fleet orchestrator with process supervision")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    // === Server ===
    /// Start the slum server
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
        #[arg(short, long, default_value = "slum.db")]
        database: String,
        /// Path to tenement.toml for process definitions
        #[arg(short, long)]
        config: Option<PathBuf>,
    },

    // === Fleet Management ===
    /// Add a server to the fleet
    ServerAdd {
        address: String,
        #[arg(short, long)]
        name: Option<String>,
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
    /// List servers
    ServerList {
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
    /// Remove a server
    ServerRemove {
        server: String,
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    // === Tenant Management ===
    /// Add a tenant
    TenantAdd {
        id: String,
        #[arg(short, long)]
        server: Option<String>,
        #[arg(short, long)]
        config: Option<String>,
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
    /// List tenants
    TenantList {
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
    /// Remove a tenant
    TenantRemove {
        id: String,
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    // === Process Supervision (from tenement) ===
    /// Spawn a process instance
    Spawn {
        process: String,
        #[arg(short, long)]
        id: String,
    },
    /// Stop a process instance
    Stop {
        /// Instance in format "process:id"
        instance: String,
    },
    /// Restart a process instance
    Restart {
        instance: String,
    },
    /// List running instances
    #[command(alias = "ls")]
    Ps,
    /// Check instance health
    Health {
        instance: String,
    },
    /// Show tenement config
    Config,

    // === Auth ===
    /// Generate a new dashboard auth token
    TokenGen {
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    // === Status ===
    /// Show fleet and instance status
    Status {
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
}
```

### 1.6 Migration Steps

1. Create workspace structure:
   ```bash
   mkdir -p slum/slum/src
   mv slum/src/* slum/slum/src/
   rmdir slum/src
   ```

2. Copy tenement:
   ```bash
   cp -r tenement/tenement slum/tenement
   ```

3. Update imports in slum crate to use `tenement::` prefix

4. Create new workspace Cargo.toml at root

5. Test: `cargo build` from workspace root

---

## Phase 2: Authentication

### 2.1 New Module: `slum/src/auth.rs`

```rust
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use axum::{
    extract::State,
    http::{header, Request, StatusCode},
    middleware::Next,
    response::Response,
};

/// Generate a random 48-character token and its Argon2 hash
pub fn generate_token() -> (String, String) {
    use rand::Rng;
    let token: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .expect("hash should succeed")
        .to_string();

    // Prefix with slum_ for easy identification
    (format!("slum_{}", token), hash)
}

/// Verify token against stored hash
pub fn verify_token(token: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(h) => h,
        Err(_) => return false,
    };

    // Strip slum_ prefix if present
    let token = token.strip_prefix("slum_").unwrap_or(token);

    Argon2::default()
        .verify_password(token.as_bytes(), &parsed)
        .is_ok()
}

/// Middleware: require Bearer token for /_/api/* routes
pub async fn auth_middleware<B>(
    State(state): State<AppState>,
    req: Request<B>,
    next: Next<B>,
) -> Result<Response, StatusCode> {
    let path = req.uri().path();

    // Only protect /_/api/* routes
    if !path.starts_with("/_/api/") {
        return Ok(next.run(req).await);
    }

    // Get stored token hash
    let token_hash = match state.db.get_config("auth_token_hash").await {
        Ok(Some(h)) => h,
        Ok(None) => return Err(StatusCode::SERVICE_UNAVAILABLE), // No token set
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };

    // Check Authorization header
    let auth_header = req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Err(StatusCode::UNAUTHORIZED),
    };

    if verify_token(token, &token_hash) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}
```

### 2.2 Token Generation Command

```rust
// In main.rs
Commands::TokenGen { database } => {
    let db = Database::open(&database).await?;

    // Check if token already exists
    if db.get_config("auth_token_hash").await?.is_some() {
        println!("Token already exists. Delete it first to regenerate.");
        println!("  sqlite3 {} \"DELETE FROM config WHERE key='auth_token_hash'\"", database);
        return Ok(());
    }

    let (token, hash) = auth::generate_token();
    db.set_config("auth_token_hash", &hash).await?;

    println!("Dashboard auth token generated:");
    println!();
    println!("  {}", token);
    println!();
    println!("Save this token securely - it cannot be retrieved later.");
    println!("Use it in the Authorization header: Bearer {}", token);
}
```

### 2.3 Database Config Table

Add to `db.rs`:

```sql
CREATE TABLE IF NOT EXISTS config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

```rust
impl Database {
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT value FROM config WHERE key = ?"
        )
        .bind(key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(v,)| v))
    }

    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO config (key, value, updated_at) VALUES (?, ?, datetime('now'))"
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
```

---

## Phase 3: Logging Infrastructure

### 3.1 Database Schema

Add to `db.rs` migrations:

```sql
-- Log entries
CREATE TABLE IF NOT EXISTS logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,        -- ISO 8601
    level TEXT NOT NULL,            -- debug, info, warn, error
    instance_id TEXT,               -- process:id (nullable for system logs)
    tenant_id TEXT,                 -- tenant (nullable)
    message TEXT NOT NULL,
    metadata TEXT,                  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS logs_fts USING fts5(
    message,
    content='logs',
    content_rowid='id'
);

-- Keep FTS in sync
CREATE TRIGGER IF NOT EXISTS logs_ai AFTER INSERT ON logs BEGIN
    INSERT INTO logs_fts(rowid, message) VALUES (new.id, new.message);
END;

CREATE TRIGGER IF NOT EXISTS logs_ad AFTER DELETE ON logs BEGIN
    INSERT INTO logs_fts(logs_fts, rowid, message) VALUES('delete', old.id, old.message);
END;

-- Indexes
CREATE INDEX IF NOT EXISTS idx_logs_ts ON logs(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_logs_instance ON logs(instance_id);
CREATE INDEX IF NOT EXISTS idx_logs_tenant ON logs(tenant_id);
CREATE INDEX IF NOT EXISTS idx_logs_level ON logs(level);
```

### 3.2 New Module: `slum/src/logs.rs`

```rust
use async_broadcast::{broadcast, Receiver, Sender};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    pub instance_id: Option<String>,
    pub tenant_id: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct LogQuery {
    pub instance_id: Option<String>,
    pub tenant_id: Option<String>,
    pub level: Option<LogLevel>,
    pub search: Option<String>,
    pub since: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 { 100 }

pub struct LogStore {
    pool: SqlitePool,
    tx: Sender<LogEntry>,
    rx: Receiver<LogEntry>,
}

impl LogStore {
    pub fn new(pool: SqlitePool) -> Self {
        let (tx, rx) = broadcast(1024);
        Self { pool, tx, rx }
    }

    /// Insert log and broadcast to SSE subscribers
    pub async fn insert(&self, mut entry: LogEntry) -> anyhow::Result<i64> {
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO logs (timestamp, level, instance_id, tenant_id, message, metadata)
            VALUES (?, ?, ?, ?, ?, ?)
            RETURNING id
            "#
        )
        .bind(entry.timestamp.to_rfc3339())
        .bind(entry.level.to_string())
        .bind(&entry.instance_id)
        .bind(&entry.tenant_id)
        .bind(&entry.message)
        .bind(entry.metadata.as_ref().map(|m| m.to_string()))
        .fetch_one(&self.pool)
        .await?;

        entry.id = id;
        let _ = self.tx.broadcast(entry).await; // Ignore if no subscribers

        Ok(id)
    }

    /// Query logs with filters
    pub async fn query(&self, q: LogQuery) -> anyhow::Result<Vec<LogEntry>> {
        // Use FTS if search is provided
        if let Some(ref search) = q.search {
            return self.query_fts(search, &q).await;
        }

        let mut sql = String::from(
            "SELECT id, timestamp, level, instance_id, tenant_id, message, metadata FROM logs WHERE 1=1"
        );

        if q.instance_id.is_some() { sql.push_str(" AND instance_id = ?"); }
        if q.tenant_id.is_some() { sql.push_str(" AND tenant_id = ?"); }
        if q.level.is_some() { sql.push_str(" AND level = ?"); }
        if q.since.is_some() { sql.push_str(" AND timestamp >= ?"); }
        if q.until.is_some() { sql.push_str(" AND timestamp <= ?"); }

        sql.push_str(" ORDER BY timestamp DESC LIMIT ? OFFSET ?");

        // Build query with bindings (simplified - real impl uses query builder)
        let rows = sqlx::query_as::<_, (i64, String, String, Option<String>, Option<String>, String, Option<String>)>(&sql)
            // ... bind parameters
            .fetch_all(&self.pool)
            .await?;

        // Map to LogEntry
        Ok(rows.into_iter().map(|r| LogEntry {
            id: r.0,
            timestamp: DateTime::parse_from_rfc3339(&r.1).unwrap().with_timezone(&Utc),
            level: serde_json::from_str(&format!("\"{}\"", r.2)).unwrap(),
            instance_id: r.3,
            tenant_id: r.4,
            message: r.5,
            metadata: r.6.and_then(|s| serde_json::from_str(&s).ok()),
        }).collect())
    }

    async fn query_fts(&self, search: &str, q: &LogQuery) -> anyhow::Result<Vec<LogEntry>> {
        let sql = r#"
            SELECT l.id, l.timestamp, l.level, l.instance_id, l.tenant_id, l.message, l.metadata
            FROM logs l
            JOIN logs_fts f ON l.id = f.rowid
            WHERE logs_fts MATCH ?
            ORDER BY l.timestamp DESC
            LIMIT ? OFFSET ?
        "#;

        // Execute and map...
        todo!()
    }

    /// Subscribe to live log stream
    pub fn subscribe(&self) -> Receiver<LogEntry> {
        self.rx.clone()
    }

    /// Delete logs older than N days
    pub async fn rotate(&self, retention_days: i64) -> anyhow::Result<u64> {
        let cutoff = Utc::now() - chrono::Duration::days(retention_days);

        let result = sqlx::query("DELETE FROM logs WHERE timestamp < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;

        // Rebuild FTS index after bulk delete
        sqlx::query("INSERT INTO logs_fts(logs_fts) VALUES('rebuild')")
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected())
    }
}
```

### 3.3 SSE Streaming Endpoint

```rust
// In dashboard.rs

use axum::response::sse::{Event, Sse};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

#[derive(Debug, Deserialize)]
pub struct LogStreamFilter {
    pub instance_id: Option<String>,
    pub tenant_id: Option<String>,
    pub level: Option<LogLevel>,
}

pub async fn logs_stream(
    State(state): State<AppState>,
    Query(filter): Query<LogStreamFilter>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let rx = state.logs.subscribe();

    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(entry) => {
                    // Apply filters
                    if let Some(ref id) = filter.instance_id {
                        if entry.instance_id.as_ref() != Some(id) {
                            continue;
                        }
                    }
                    if let Some(ref id) = filter.tenant_id {
                        if entry.tenant_id.as_ref() != Some(id) {
                            continue;
                        }
                    }
                    if let Some(level) = filter.level {
                        if entry.level != level {
                            continue;
                        }
                    }

                    let json = serde_json::to_string(&entry).unwrap();
                    yield Ok(Event::default().data(json));
                }
                Err(_) => break, // Channel closed
            }
        }
    };

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
    )
}
```

---

## Phase 4: Metrics Collection

### 4.1 Database Schema

```sql
-- Metrics snapshots (flushed every minute)
CREATE TABLE IF NOT EXISTS metrics_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    name TEXT NOT NULL,
    value REAL NOT NULL,
    labels TEXT,  -- JSON
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_metrics_ts ON metrics_history(timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_metrics_name ON metrics_history(name, timestamp DESC);
```

### 4.2 New Module: `slum/src/metrics.rs`

```rust
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::Serialize;

const RING_SIZE: usize = 3600; // 1 hour at 1 sample/sec

#[derive(Debug, Clone, Serialize)]
pub struct MetricSnapshot {
    pub name: String,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub timestamp: i64,
}

pub struct MetricsCollector {
    counters: RwLock<HashMap<String, u64>>,
    histograms: RwLock<HashMap<String, Vec<f64>>>,
}

impl MetricsCollector {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            counters: RwLock::new(HashMap::new()),
            histograms: RwLock::new(HashMap::new()),
        })
    }

    /// Increment counter
    pub async fn inc(&self, name: &str, labels: &HashMap<String, String>) {
        let key = make_key(name, labels);
        let mut counters = self.counters.write().await;
        *counters.entry(key).or_insert(0) += 1;
    }

    /// Add value (for counters that increment by N)
    pub async fn add(&self, name: &str, labels: &HashMap<String, String>, n: u64) {
        let key = make_key(name, labels);
        let mut counters = self.counters.write().await;
        *counters.entry(key).or_insert(0) += n;
    }

    /// Record latency observation
    pub async fn observe(&self, name: &str, labels: &HashMap<String, String>, value: f64) {
        let key = make_key(name, labels);
        let mut histograms = self.histograms.write().await;
        let hist = histograms.entry(key).or_insert_with(Vec::new);
        hist.push(value);

        // Keep last 1000 samples per histogram
        if hist.len() > 1000 {
            hist.remove(0);
        }
    }

    /// Get counter value
    pub async fn get_counter(&self, name: &str, labels: &HashMap<String, String>) -> u64 {
        let key = make_key(name, labels);
        let counters = self.counters.read().await;
        *counters.get(&key).unwrap_or(&0)
    }

    /// Get histogram summary (p50, p95, p99, mean)
    pub async fn get_histogram_summary(
        &self,
        name: &str,
        labels: &HashMap<String, String>,
    ) -> Option<HistogramSummary> {
        let key = make_key(name, labels);
        let histograms = self.histograms.read().await;
        let hist = histograms.get(&key)?;

        if hist.is_empty() {
            return None;
        }

        let mut sorted = hist.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        Some(HistogramSummary {
            count: sorted.len(),
            min: sorted[0],
            max: sorted[sorted.len() - 1],
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: percentile(&sorted, 50.0),
            p95: percentile(&sorted, 95.0),
            p99: percentile(&sorted, 99.0),
        })
    }

    /// Get all current metrics as snapshots
    pub async fn snapshot_all(&self) -> Vec<MetricSnapshot> {
        let now = chrono::Utc::now().timestamp_millis();
        let mut snapshots = Vec::new();

        let counters = self.counters.read().await;
        for (key, value) in counters.iter() {
            let (name, labels) = parse_key(key);
            snapshots.push(MetricSnapshot {
                name,
                value: *value as f64,
                labels,
                timestamp: now,
            });
        }

        snapshots
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HistogramSummary {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}

fn make_key(name: &str, labels: &HashMap<String, String>) -> String {
    let mut parts: Vec<_> = labels.iter().collect();
    parts.sort_by_key(|(k, _)| *k);
    let label_str: String = parts
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join(",");
    format!("{}|{}", name, label_str)
}

fn parse_key(key: &str) -> (String, HashMap<String, String>) {
    let parts: Vec<&str> = key.splitn(2, '|').collect();
    let name = parts[0].to_string();
    let labels = if parts.len() > 1 && !parts[1].is_empty() {
        parts[1]
            .split(',')
            .filter_map(|p| {
                let kv: Vec<&str> = p.splitn(2, '=').collect();
                if kv.len() == 2 {
                    Some((kv[0].to_string(), kv[1].to_string()))
                } else {
                    None
                }
            })
            .collect()
    } else {
        HashMap::new()
    };
    (name, labels)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((p / 100.0) * (sorted.len() - 1) as f64) as usize;
    sorted[idx.min(sorted.len() - 1)]
}
```

### 4.3 Request Metrics Middleware

```rust
use axum::{
    extract::State,
    http::Request,
    middleware::Next,
    response::Response,
};
use std::time::Instant;

pub async fn metrics_middleware<B>(
    State(state): State<AppState>,
    req: Request<B>,
    next: Next<B>,
) -> Response {
    let start = Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();

    // Extract tenant if present
    let tenant = req.headers()
        .get("X-Tenant-ID")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let latency_ms = start.elapsed().as_secs_f64() * 1000.0;

    // Build labels
    let mut labels = HashMap::new();
    labels.insert("method".into(), method);
    labels.insert("status".into(), status.to_string());

    // Normalize path for metrics (avoid cardinality explosion)
    let path_normalized = normalize_path(&path);
    labels.insert("path".into(), path_normalized);

    if let Some(t) = tenant {
        labels.insert("tenant".into(), t);
    }

    // Record metrics
    state.metrics.inc("http_requests_total", &labels).await;
    state.metrics.observe("http_request_duration_ms", &labels, latency_ms).await;

    if status >= 500 {
        state.metrics.inc("http_errors_total", &labels).await;
    }

    response
}

/// Normalize path to avoid cardinality explosion
/// /api/tenants/abc123 -> /api/tenants/:id
fn normalize_path(path: &str) -> String {
    let segments: Vec<&str> = path.split('/').collect();
    segments
        .iter()
        .map(|s| {
            // Replace UUIDs and numeric IDs with placeholder
            if s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4 {
                ":id"
            } else if s.chars().all(|c| c.is_ascii_digit()) && !s.is_empty() {
                ":id"
            } else {
                *s
            }
        })
        .collect::<Vec<_>>()
        .join("/")
}
```

---

## Phase 5: Dashboard (Svelte SPA)

### 5.1 Package Configuration

**`dashboard/package.json`**

```json
{
  "name": "slum-dashboard",
  "version": "0.2.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite dev --port 5173",
    "build": "vite build",
    "preview": "vite preview"
  },
  "devDependencies": {
    "@sveltejs/adapter-static": "^3.0.0",
    "@sveltejs/kit": "^2.0.0",
    "@sveltejs/vite-plugin-svelte": "^4.0.0",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0",
    "svelte": "^5.0.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.0.0",
    "vite": "^6.0.0"
  }
}
```

**`dashboard/svelte.config.js`**

```javascript
import adapter from '@sveltejs/adapter-static';

export default {
  kit: {
    adapter: adapter({
      pages: 'dist',
      assets: 'dist',
      fallback: 'index.html',
    }),
    paths: {
      base: '/_',
    },
  },
};
```

**`dashboard/tailwind.config.js`**

```javascript
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {},
  },
  plugins: [],
};
```

### 5.2 API Client

**`dashboard/src/lib/api.ts`**

```typescript
const BASE = '/_/api';

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const token = localStorage.getItem('slum_token');

  const res = await fetch(`${BASE}${path}`, {
    ...options,
    headers: {
      'Content-Type': 'application/json',
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...options.headers,
    },
  });

  if (res.status === 401) {
    // Redirect to login or show auth modal
    throw new Error('Unauthorized');
  }

  if (!res.ok) {
    throw new Error(`API error: ${res.status}`);
  }

  return res.json();
}

export const api = {
  // Overview
  getOverview: () => request<Overview>('/overview'),

  // Servers
  getServers: () => request<Server[]>('/servers'),
  getServer: (id: string) => request<Server>(`/servers/${id}`),
  deleteServer: (id: string) => request(`/servers/${id}`, { method: 'DELETE' }),

  // Tenants
  getTenants: () => request<Tenant[]>('/tenants'),
  getTenant: (id: string) => request<Tenant>(`/tenants/${id}`),
  createTenant: (data: CreateTenant) => request<Tenant>('/tenants', {
    method: 'POST',
    body: JSON.stringify(data),
  }),
  deleteTenant: (id: string) => request(`/tenants/${id}`, { method: 'DELETE' }),

  // Instances
  getInstances: () => request<Instance[]>('/instances'),
  restartInstance: (id: string) => request(`/instances/${id}/restart`, { method: 'POST' }),
  stopInstance: (id: string) => request(`/instances/${id}/stop`, { method: 'POST' }),

  // Logs
  getLogs: (params?: LogQuery) => {
    const qs = new URLSearchParams(params as any).toString();
    return request<LogEntry[]>(`/logs${qs ? `?${qs}` : ''}`);
  },

  // Metrics
  getMetrics: () => request<MetricSnapshot[]>('/metrics'),
};

// Types
export interface Overview {
  servers: number;
  tenants: number;
  instances: number;
  healthy: number;
  unhealthy: number;
  recent_logs: LogEntry[];
}

export interface Server {
  id: string;
  name: string;
  address: string;
  tenant_count: number;
  created_at: string;
}

export interface Tenant {
  id: string;
  server_id: string;
  config: string | null;
  status: string;
  created_at: string;
}

export interface Instance {
  id: string;
  socket: string;
  uptime_secs: number;
  restarts: number;
  health: string;
  status: string;
}

export interface LogEntry {
  id: number;
  timestamp: string;
  level: 'debug' | 'info' | 'warn' | 'error';
  instance_id: string | null;
  tenant_id: string | null;
  message: string;
}

export interface LogQuery {
  instance_id?: string;
  tenant_id?: string;
  level?: string;
  search?: string;
  limit?: number;
}

export interface MetricSnapshot {
  name: string;
  value: number;
  labels: Record<string, string>;
}

export interface CreateTenant {
  id: string;
  server?: string;
  config?: string;
}
```

### 5.3 Layout Component

**`dashboard/src/routes/+layout.svelte`**

```svelte
<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';

  const nav = [
    { href: '/_/', label: 'Overview', icon: '📊' },
    { href: '/_/servers', label: 'Servers', icon: '🖥️' },
    { href: '/_/tenants', label: 'Tenants', icon: '👥' },
    { href: '/_/instances', label: 'Instances', icon: '⚙️' },
    { href: '/_/logs', label: 'Logs', icon: '📋' },
  ];

  let token = $state('');
  let showTokenModal = $state(false);

  function saveToken() {
    localStorage.setItem('slum_token', token);
    showTokenModal = false;
    location.reload();
  }
</script>

<div class="min-h-screen bg-gray-900 text-gray-100 flex">
  <!-- Sidebar -->
  <nav class="w-48 bg-gray-800 border-r border-gray-700 flex flex-col">
    <div class="p-4 border-b border-gray-700">
      <h1 class="text-lg font-bold">Slum</h1>
      <p class="text-xs text-gray-500">Fleet Orchestrator</p>
    </div>

    <ul class="flex-1 py-2">
      {#each nav as item}
        <li>
          <a
            href={item.href}
            class="flex items-center gap-2 px-4 py-2 hover:bg-gray-700 transition-colors
                   {$page.url.pathname === item.href ? 'bg-gray-700 border-l-2 border-blue-500' : ''}"
          >
            <span>{item.icon}</span>
            <span>{item.label}</span>
          </a>
        </li>
      {/each}
    </ul>

    <div class="p-4 border-t border-gray-700">
      <button
        onclick={() => showTokenModal = true}
        class="text-xs text-gray-500 hover:text-gray-300"
      >
        Set Token
      </button>
    </div>
  </nav>

  <!-- Main content -->
  <main class="flex-1 p-6 overflow-auto">
    <slot />
  </main>
</div>

<!-- Token Modal -->
{#if showTokenModal}
  <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
    <div class="bg-gray-800 rounded-lg p-6 w-96">
      <h2 class="text-lg font-bold mb-4">Set Auth Token</h2>
      <input
        type="password"
        bind:value={token}
        placeholder="slum_..."
        class="w-full bg-gray-700 border border-gray-600 rounded px-3 py-2 mb-4"
      />
      <div class="flex gap-2 justify-end">
        <button
          onclick={() => showTokenModal = false}
          class="px-4 py-2 text-gray-400 hover:text-white"
        >
          Cancel
        </button>
        <button
          onclick={saveToken}
          class="px-4 py-2 bg-blue-600 rounded hover:bg-blue-500"
        >
          Save
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  :global(body) {
    @apply bg-gray-900;
  }
</style>
```

### 5.4 Overview Page

**`dashboard/src/routes/+page.svelte`**

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Overview, type LogEntry } from '$lib/api';

  let data = $state<Overview | null>(null);
  let error = $state('');

  onMount(async () => {
    try {
      data = await api.getOverview();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Failed to load';
    }
  });
</script>

<h1 class="text-2xl font-bold mb-6">Overview</h1>

{#if error}
  <div class="bg-red-900/50 border border-red-500 rounded p-4 mb-6">
    {error}
  </div>
{:else if !data}
  <div class="animate-pulse text-gray-500">Loading...</div>
{:else}
  <!-- Stats Grid -->
  <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
    <div class="bg-gray-800 rounded-lg p-4">
      <div class="text-3xl font-bold">{data.servers}</div>
      <div class="text-gray-500 text-sm">Servers</div>
    </div>
    <div class="bg-gray-800 rounded-lg p-4">
      <div class="text-3xl font-bold">{data.tenants}</div>
      <div class="text-gray-500 text-sm">Tenants</div>
    </div>
    <div class="bg-gray-800 rounded-lg p-4">
      <div class="text-3xl font-bold text-green-400">{data.healthy}</div>
      <div class="text-gray-500 text-sm">Healthy</div>
    </div>
    <div class="bg-gray-800 rounded-lg p-4">
      <div class="text-3xl font-bold text-red-400">{data.unhealthy}</div>
      <div class="text-gray-500 text-sm">Unhealthy</div>
    </div>
  </div>

  <!-- Recent Logs -->
  <h2 class="text-lg font-semibold mb-3">Recent Logs</h2>
  <div class="bg-gray-800 rounded-lg overflow-hidden">
    {#each data.recent_logs as log}
      <div class="px-4 py-2 border-b border-gray-700 font-mono text-sm flex gap-3">
        <span class="text-gray-500 w-20 shrink-0">
          {new Date(log.timestamp).toLocaleTimeString()}
        </span>
        <span class="w-12 shrink-0 {
          log.level === 'error' ? 'text-red-400' :
          log.level === 'warn' ? 'text-yellow-400' :
          log.level === 'debug' ? 'text-gray-500' : 'text-blue-400'
        }">
          {log.level}
        </span>
        {#if log.instance_id}
          <span class="text-purple-400 shrink-0">[{log.instance_id}]</span>
        {/if}
        <span class="text-gray-300 truncate">{log.message}</span>
      </div>
    {/each}
    {#if data.recent_logs.length === 0}
      <div class="px-4 py-8 text-center text-gray-500">No recent logs</div>
    {/if}
  </div>
{/if}
```

### 5.5 Logs Page with SSE Streaming

**`dashboard/src/routes/logs/+page.svelte`**

```svelte
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { api, type LogEntry } from '$lib/api';

  let logs = $state<LogEntry[]>([]);
  let streaming = $state(false);
  let eventSource: EventSource | null = null;

  let filter = $state({
    instance_id: '',
    tenant_id: '',
    search: '',
  });

  onMount(async () => {
    logs = await api.getLogs({ limit: 100 });
  });

  onDestroy(() => {
    stopStream();
  });

  function startStream() {
    const params = new URLSearchParams();
    if (filter.instance_id) params.set('instance_id', filter.instance_id);
    if (filter.tenant_id) params.set('tenant_id', filter.tenant_id);

    const token = localStorage.getItem('slum_token');
    // Note: SSE doesn't support Authorization header, use query param
    if (token) params.set('token', token);

    eventSource = new EventSource(`/_/api/logs/stream?${params}`);

    eventSource.onmessage = (event) => {
      const log = JSON.parse(event.data) as LogEntry;
      logs = [log, ...logs.slice(0, 499)];
    };

    eventSource.onerror = () => {
      stopStream();
    };

    streaming = true;
  }

  function stopStream() {
    eventSource?.close();
    eventSource = null;
    streaming = false;
  }

  async function search() {
    stopStream();
    logs = await api.getLogs({
      instance_id: filter.instance_id || undefined,
      tenant_id: filter.tenant_id || undefined,
      search: filter.search || undefined,
      limit: 200,
    });
  }
</script>

<div class="flex items-center justify-between mb-6">
  <h1 class="text-2xl font-bold">Logs</h1>

  <div class="flex gap-2">
    <input
      type="text"
      placeholder="Instance ID"
      bind:value={filter.instance_id}
      class="bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm w-32"
    />
    <input
      type="text"
      placeholder="Search..."
      bind:value={filter.search}
      class="bg-gray-800 border border-gray-700 rounded px-3 py-1.5 text-sm w-48"
    />
    <button
      onclick={search}
      class="px-3 py-1.5 bg-gray-700 rounded hover:bg-gray-600 text-sm"
    >
      Search
    </button>
    <button
      onclick={() => streaming ? stopStream() : startStream()}
      class="px-3 py-1.5 rounded text-sm {streaming ? 'bg-red-600 hover:bg-red-500' : 'bg-blue-600 hover:bg-blue-500'}"
    >
      {streaming ? 'Stop' : 'Stream'}
    </button>
  </div>
</div>

<div class="bg-gray-800 rounded-lg font-mono text-xs overflow-auto" style="max-height: calc(100vh - 180px)">
  {#each logs as log (log.id)}
    <div class="px-3 py-1 border-b border-gray-700/50 hover:bg-gray-750 flex gap-2">
      <span class="text-gray-500 w-20 shrink-0">
        {new Date(log.timestamp).toLocaleTimeString()}
      </span>
      <span class="w-10 shrink-0 uppercase {
        log.level === 'error' ? 'text-red-400' :
        log.level === 'warn' ? 'text-yellow-400' :
        log.level === 'debug' ? 'text-gray-500' : 'text-blue-400'
      }">
        {log.level}
      </span>
      {#if log.instance_id}
        <span class="text-purple-400 shrink-0">[{log.instance_id}]</span>
      {/if}
      <span class="text-gray-300">{log.message}</span>
    </div>
  {/each}

  {#if logs.length === 0}
    <div class="px-4 py-12 text-center text-gray-500">
      No logs found
    </div>
  {/if}
</div>
```

### 5.6 Instances Page

**`dashboard/src/routes/instances/+page.svelte`**

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { api, type Instance } from '$lib/api';

  let instances = $state<Instance[]>([]);
  let loading = $state(true);

  onMount(async () => {
    instances = await api.getInstances();
    loading = false;
  });

  async function restart(id: string) {
    await api.restartInstance(id);
    instances = await api.getInstances();
  }

  async function stop(id: string) {
    await api.stopInstance(id);
    instances = await api.getInstances();
  }

  function formatUptime(secs: number): string {
    if (secs < 60) return `${secs}s`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m`;
    if (secs < 86400) return `${Math.floor(secs / 3600)}h`;
    return `${Math.floor(secs / 86400)}d`;
  }
</script>

<h1 class="text-2xl font-bold mb-6">Process Instances</h1>

{#if loading}
  <div class="animate-pulse text-gray-500">Loading...</div>
{:else if instances.length === 0}
  <div class="bg-gray-800 rounded-lg p-8 text-center text-gray-500">
    No instances running
  </div>
{:else}
  <div class="bg-gray-800 rounded-lg overflow-hidden">
    <table class="w-full">
      <thead class="bg-gray-750 text-left text-sm text-gray-400">
        <tr>
          <th class="px-4 py-3">Instance</th>
          <th class="px-4 py-3">Socket</th>
          <th class="px-4 py-3">Uptime</th>
          <th class="px-4 py-3">Restarts</th>
          <th class="px-4 py-3">Health</th>
          <th class="px-4 py-3">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each instances as inst}
          <tr class="border-t border-gray-700">
            <td class="px-4 py-3 font-mono">{inst.id}</td>
            <td class="px-4 py-3 text-gray-400 text-sm truncate max-w-xs">{inst.socket}</td>
            <td class="px-4 py-3">{formatUptime(inst.uptime_secs)}</td>
            <td class="px-4 py-3">{inst.restarts}</td>
            <td class="px-4 py-3">
              <span class="px-2 py-0.5 rounded text-xs {
                inst.health === 'healthy' ? 'bg-green-900 text-green-300' :
                inst.health === 'unhealthy' ? 'bg-red-900 text-red-300' :
                inst.health === 'degraded' ? 'bg-yellow-900 text-yellow-300' :
                'bg-gray-700 text-gray-300'
              }">
                {inst.health}
              </span>
            </td>
            <td class="px-4 py-3">
              <button
                onclick={() => restart(inst.id)}
                class="text-blue-400 hover:text-blue-300 text-sm mr-3"
              >
                Restart
              </button>
              <button
                onclick={() => stop(inst.id)}
                class="text-red-400 hover:text-red-300 text-sm"
              >
                Stop
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}
```

---

## Phase 6: Route Organization

### 6.1 Dashboard Routes Module

**`slum/src/dashboard.rs`**

```rust
use axum::{
    routing::{get, post, delete},
    Router,
    extract::{State, Path, Query},
    response::{Html, IntoResponse, Sse},
    Json,
};
use rust_embed::RustEmbed;

use crate::{AppState, logs::LogQuery};

#[derive(RustEmbed)]
#[folder = "../dashboard/dist"]
struct Assets;

pub fn routes() -> Router<AppState> {
    Router::new()
        // Static assets
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/*path", get(static_file))

        // API endpoints
        .route("/api/overview", get(api_overview))
        .route("/api/servers", get(api_servers))
        .route("/api/servers/:id", get(api_server).delete(api_server_delete))
        .route("/api/tenants", get(api_tenants).post(api_tenant_create))
        .route("/api/tenants/:id", get(api_tenant).delete(api_tenant_delete))
        .route("/api/instances", get(api_instances))
        .route("/api/instances/:id/restart", post(api_instance_restart))
        .route("/api/instances/:id/stop", post(api_instance_stop))
        .route("/api/logs", get(api_logs))
        .route("/api/logs/stream", get(api_logs_stream))
        .route("/api/metrics", get(api_metrics))
}

async fn index() -> impl IntoResponse {
    match Assets::get("index.html") {
        Some(content) => Html(String::from_utf8_lossy(&content.data).to_string()).into_response(),
        None => (axum::http::StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

async fn static_file(Path(path): Path<String>) -> impl IntoResponse {
    let path = path.trim_start_matches('/');

    // Try exact path first
    if let Some(content) = Assets::get(path) {
        let mime = mime_guess::from_path(path).first_or_octet_stream();
        return (
            [(axum::http::header::CONTENT_TYPE, mime.as_ref())],
            content.data.to_vec(),
        ).into_response();
    }

    // SPA fallback - serve index.html for routes
    if !path.contains('.') {
        if let Some(content) = Assets::get("index.html") {
            return Html(String::from_utf8_lossy(&content.data).to_string()).into_response();
        }
    }

    (axum::http::StatusCode::NOT_FOUND, "Not found").into_response()
}

// API handlers...
async fn api_overview(State(state): State<AppState>) -> Json<Overview> {
    let servers = state.db.list_servers().await.unwrap_or_default();
    let tenants = state.db.list_tenants().await.unwrap_or_default();

    let (healthy, unhealthy) = if let Some(ref hyp) = state.hypervisor {
        let instances = hyp.list().await;
        let h = instances.iter().filter(|i| i.health == "healthy").count();
        let u = instances.len() - h;
        (h, u)
    } else {
        (0, 0)
    };

    let recent_logs = state.logs.query(LogQuery {
        limit: 10,
        ..Default::default()
    }).await.unwrap_or_default();

    Json(Overview {
        servers: servers.len(),
        tenants: tenants.len(),
        instances: healthy + unhealthy,
        healthy,
        unhealthy,
        recent_logs,
    })
}

// ... other handlers
```

### 6.2 Main Server Setup

**`slum/src/main.rs` (serve function)**

```rust
async fn serve(port: u16, database: &str, config: Option<PathBuf>) -> Result<()> {
    // Initialize database
    let db = Database::open(database).await?;

    // Initialize hypervisor if config provided
    let hypervisor = match config {
        Some(path) => Some(Arc::new(tenement::Hypervisor::from_config_path(&path)?)),
        None => tenement::Hypervisor::from_config_file().ok().map(Arc::new),
    };

    // Initialize logging
    let logs = Arc::new(logs::LogStore::new(db.pool.clone()));

    // Initialize metrics
    let metrics = metrics::MetricsCollector::new();

    // Build state
    let state = AppState {
        db: Arc::new(db),
        hypervisor,
        logs,
        metrics,
    };

    // Start hypervisor health monitor if present
    if let Some(ref hyp) = state.hypervisor {
        hyp.clone().start_monitor();
    }

    // Start log rotation task (daily)
    {
        let logs = state.logs.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(86400));
            loop {
                interval.tick().await;
                if let Err(e) = logs.rotate(7).await {
                    tracing::error!("Log rotation failed: {}", e);
                }
            }
        });
    }

    // Build router
    let app = Router::new()
        // Public fleet API
        .route("/api/health", get(api::health))
        .route("/api/servers", get(api::list_servers).post(api::add_server))
        .route("/api/servers/:id", delete(api::remove_server))
        .route("/api/tenants", get(api::list_tenants).post(api::add_tenant))
        .route("/api/tenants/:id", delete(api::remove_tenant))

        // Dashboard (under /_/)
        .nest("/_", dashboard::routes())

        // Middleware
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            auth::auth_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics::metrics_middleware,
        ))

        // Fallback: proxy to tenant
        .fallback(proxy::handle_request)

        .layer(TraceLayer::new_for_http())
        .with_state(state);

    // Start server
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("slum listening on http://0.0.0.0:{}", port);
    tracing::info!("Dashboard available at http://localhost:{}/_/", port);

    axum::serve(listener, app).await?;
    Ok(())
}
```

### 6.3 AppState Definition

```rust
#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub hypervisor: Option<Arc<tenement::Hypervisor>>,
    pub logs: Arc<logs::LogStore>,
    pub metrics: Arc<metrics::MetricsCollector>,
}
```

---

## Phase 7: Build Process

### 7.1 Updated Makefile

```makefile
.PHONY: all build release dashboard test clean install publish help

all: build

# Build dashboard first, then Rust
dashboard:
	cd dashboard && npm install && npm run build

build: dashboard
	cargo build

release: dashboard
	cargo build --release

# Development
dev:
	cargo watch -x build

dev-dashboard:
	cd dashboard && npm run dev

# Testing
test:
	cargo test

test-all:
	cargo test
	cd dashboard && npm test

# Clean
clean:
	cargo clean
	rm -rf dashboard/dist dashboard/node_modules

# Install
install: release
	cargo install --path slum

# Publishing
publish: release
	cd slum && cargo publish
	cd tenement && cargo publish

publish-pypi: release
	maturin publish

# Help
help:
	@echo "Targets:"
	@echo "  build      - Build debug (includes dashboard)"
	@echo "  release    - Build release (includes dashboard)"
	@echo "  dashboard  - Build dashboard only"
	@echo "  dev        - Watch and rebuild Rust"
	@echo "  dev-dashboard - Run dashboard dev server"
	@echo "  test       - Run tests"
	@echo "  clean      - Clean all artifacts"
	@echo "  install    - Install release binary"
	@echo "  publish    - Publish to crates.io"
	@echo "  publish-pypi - Publish to PyPI"

.DEFAULT_GOAL := help
```

---

## Phase 8: Testing Strategy

### 8.1 Unit Tests

Each new module should have tests:

```rust
// slum/src/auth.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let (token, hash) = generate_token();
        assert!(token.starts_with("slum_"));
        assert!(token.len() > 40);
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_token_verification() {
        let (token, hash) = generate_token();
        assert!(verify_token(&token, &hash));
        assert!(!verify_token("wrong_token", &hash));
    }
}

// slum/src/logs.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_log_insert_and_query() {
        // Create temp database
        // Insert logs
        // Query and verify
    }

    #[tokio::test]
    async fn test_fts_search() {
        // Insert logs with various messages
        // Search for keywords
        // Verify results
    }
}

// slum/src/metrics.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_counter_increment() {
        let m = MetricsCollector::new();
        let labels = HashMap::new();

        m.inc("test", &labels).await;
        m.inc("test", &labels).await;

        assert_eq!(m.get_counter("test", &labels).await, 2);
    }

    #[tokio::test]
    async fn test_histogram_percentiles() {
        let m = MetricsCollector::new();
        let labels = HashMap::new();

        for i in 1..=100 {
            m.observe("latency", &labels, i as f64).await;
        }

        let summary = m.get_histogram_summary("latency", &labels).await.unwrap();
        assert_eq!(summary.p50, 50.0);
        assert_eq!(summary.p99, 99.0);
    }
}
```

### 8.2 Integration Tests

**`slum/tests/integration.rs`**

```rust
use slum::{Database, AppState};
use axum::http::StatusCode;
use axum_test::TestServer;

#[tokio::test]
async fn test_auth_required() {
    let app = create_test_app().await;
    let server = TestServer::new(app).unwrap();

    // Dashboard API requires auth
    let res = server.get("/_/api/overview").await;
    assert_eq!(res.status_code(), StatusCode::UNAUTHORIZED);

    // With valid token
    let res = server
        .get("/_/api/overview")
        .add_header("Authorization", "Bearer slum_testtoken")
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_log_streaming() {
    // Connect SSE
    // Insert log entry
    // Verify it arrives via stream
}

#[tokio::test]
async fn test_metrics_recording() {
    // Make requests
    // Check metrics endpoint
    // Verify counters incremented
}
```

---

## Phase 9: Migration Path

### 9.1 For Existing Slum Users

1. **Backup database**: `cp slum.db slum.db.backup`

2. **Upgrade binary**: Download new version or `cargo install slum`

3. **Generate auth token**:
   ```bash
   slum token-gen
   # Save the displayed token securely
   ```

4. **Start server**: `slum serve` (same as before)

5. **Access dashboard**: `http://localhost:8080/_/`

### 9.2 For Existing Tenement Users

1. **Create tenement.toml** if not exists (same format)

2. **Start slum with config**:
   ```bash
   slum serve --config tenement.toml
   ```

3. **Use new CLI**:
   ```bash
   slum spawn api --id user123    # was: tenement spawn api --id user123
   slum ps                         # was: tenement ps
   slum stop api:user123          # was: tenement stop api:user123
   ```

### 9.3 Database Migrations

New tables are created automatically on first run. Existing tables are not modified.

```sql
-- Auto-created on startup if not exist:
-- config (for auth token)
-- logs (with FTS5)
-- metrics_history
```

---

## Implementation Timeline

| Phase | Description | Effort | Dependencies |
|-------|-------------|--------|--------------|
| 1 | Merge tenement into workspace | 2 days | None |
| 2 | Authentication | 0.5 days | Phase 1 |
| 3 | Logging infrastructure | 1.5 days | Phase 1 |
| 4 | Metrics collection | 1 day | Phase 1 |
| 5 | Dashboard UI | 2 days | Phases 2, 3, 4 |
| 6 | Route organization | 0.5 days | All above |
| 7 | Build process | 0.5 days | Phase 5 |
| 8 | Testing | 1 day | All above |
| 9 | Documentation | 0.5 days | All above |

**Total: ~9-10 days**

---

## Verification Checklist

After implementation, verify:

- [ ] `cargo build` succeeds from workspace root
- [ ] `cargo test` passes all tests
- [ ] `slum serve` starts without errors
- [ ] `slum token-gen` generates and stores token
- [ ] Dashboard loads at `/_/`
- [ ] Dashboard API requires auth token
- [ ] Log streaming works via SSE
- [ ] Metrics are recorded for requests
- [ ] Process instances can be spawned/stopped via CLI
- [ ] Process instances appear in dashboard
- [ ] Python bindings still work (`pip install` and `import slum`)
- [ ] Existing fleet management API unchanged
- [ ] Proxy routing still works

---

## Files to Create/Modify

### New Files
- `slum/Cargo.toml` (workspace root)
- `slum/slum/Cargo.toml` (crate)
- `slum/slum/src/auth.rs`
- `slum/slum/src/logs.rs`
- `slum/slum/src/metrics.rs`
- `slum/slum/src/dashboard.rs`
- `slum/tenement/` (entire directory from tenement repo)
- `slum/dashboard/` (entire Svelte app)

### Modified Files
- `slum/slum/src/main.rs` (unified CLI)
- `slum/slum/src/db.rs` (new tables)
- `slum/slum/src/lib.rs` (new exports)
- `slum/Makefile` (dashboard build)

### Deleted/Moved
- Current `slum/src/` → `slum/slum/src/`
- Current `slum/Cargo.toml` → becomes workspace member config
