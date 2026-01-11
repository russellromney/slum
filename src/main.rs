mod db;
mod proxy;
mod api;

use anyhow::Result;
use axum::{
    routing::{get, delete},
    Router,
};
use clap::{Parser, Subcommand};
use std::sync::Arc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::db::Database;

#[derive(Parser)]
#[command(name = "slum")]
#[command(about = "Fleet orchestrator for tenement servers")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the slum server
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// Add a tenement server to the fleet
    ServerAdd {
        /// Server address (e.g., "10.0.0.1:9000" or "tenement-1.internal:9000")
        address: String,

        /// Server name (optional, defaults to address)
        #[arg(short, long)]
        name: Option<String>,

        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// List servers in the fleet
    ServerList {
        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// Remove a server from the fleet
    ServerRemove {
        /// Server ID or name
        server: String,

        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// Add a tenant
    TenantAdd {
        /// Tenant ID (e.g., "romneys")
        id: String,

        /// Server to place tenant on (ID or name). If not specified, picks server with capacity.
        #[arg(short, long)]
        server: Option<String>,

        /// Tenant config as JSON (passed to tenement)
        #[arg(short, long)]
        config: Option<String>,

        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// List tenants
    TenantList {
        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// Remove a tenant
    TenantRemove {
        /// Tenant ID
        id: String,

        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },

    /// Show fleet status
    Status {
        /// Database path
        #[arg(short, long, default_value = "slum.db")]
        database: String,
    },
}

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "slum=info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { port, database } => {
            serve(port, &database).await?;
        }
        Commands::ServerAdd { address, name, database } => {
            let db = Database::open(&database).await?;
            let name = name.unwrap_or_else(|| address.clone());
            let server = db.add_server(&name, &address).await?;
            println!("Added server: {} ({})", server.name, server.id);
        }
        Commands::ServerList { database } => {
            let db = Database::open(&database).await?;
            let servers = db.list_servers().await?;
            if servers.is_empty() {
                println!("No servers in fleet");
            } else {
                println!("{:<36} {:<20} {:<30} {:<10}", "ID", "NAME", "ADDRESS", "TENANTS");
                for s in servers {
                    println!("{:<36} {:<20} {:<30} {:<10}", s.id, s.name, s.address, s.tenant_count);
                }
            }
        }
        Commands::ServerRemove { server, database } => {
            let db = Database::open(&database).await?;
            db.remove_server(&server).await?;
            println!("Removed server: {}", server);
        }
        Commands::TenantAdd { id, server, config, database } => {
            let db = Database::open(&database).await?;
            let tenant = db.add_tenant(&id, server.as_deref(), config.as_deref()).await?;
            println!("Added tenant: {} on server {}", tenant.id, tenant.server_id);
        }
        Commands::TenantList { database } => {
            let db = Database::open(&database).await?;
            let tenants = db.list_tenants().await?;
            if tenants.is_empty() {
                println!("No tenants");
            } else {
                println!("{:<20} {:<36} {:<10}", "ID", "SERVER", "STATUS");
                for t in tenants {
                    println!("{:<20} {:<36} {:<10}", t.id, t.server_id, t.status);
                }
            }
        }
        Commands::TenantRemove { id, database } => {
            let db = Database::open(&database).await?;
            db.remove_tenant(&id).await?;
            println!("Removed tenant: {}", id);
        }
        Commands::Status { database } => {
            let db = Database::open(&database).await?;
            let servers = db.list_servers().await?;
            let tenants = db.list_tenants().await?;
            println!("Fleet Status:");
            println!("  Servers: {}", servers.len());
            println!("  Tenants: {}", tenants.len());
            println!();
            if !servers.is_empty() {
                println!("Servers:");
                for s in &servers {
                    println!("  {} ({}) - {} tenants", s.name, s.address, s.tenant_count);
                }
            }
        }
    }

    Ok(())
}

async fn serve(port: u16, database: &str) -> Result<()> {
    let db = Database::open(database).await?;
    let state = AppState { db: Arc::new(db) };

    let app = Router::new()
        // Management API
        .route("/api/health", get(api::health))
        .route("/api/servers", get(api::list_servers).post(api::add_server))
        .route("/api/servers/{id}", delete(api::remove_server))
        .route("/api/tenants", get(api::list_tenants).post(api::add_tenant))
        .route("/api/tenants/{id}", delete(api::remove_tenant))
        // Catch-all: proxy to tenant
        .fallback(proxy::handle_request)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port)).await?;
    tracing::info!("slum listening on port {}", port);

    axum::serve(listener, app).await?;
    Ok(())
}
