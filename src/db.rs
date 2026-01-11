use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};

#[derive(Clone)]
pub struct Database {
    pool: Pool<Sqlite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub address: String,
    pub tenant_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub server_id: String,
    pub config: Option<String>,
    pub status: String,
    pub created_at: String,
}

impl Database {
    pub async fn open(path: &str) -> Result<Self> {
        let url = format!("sqlite:{}?mode=rwc", path);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&url)
            .await?;

        // Run migrations
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS servers (
                id TEXT PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                address TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL REFERENCES servers(id),
                config TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS domain_aliases (
                domain TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL REFERENCES tenants(id)
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    // Server operations

    pub async fn add_server(&self, name: &str, address: &str) -> Result<Server> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO servers (id, name, address, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(address)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(Server {
            id,
            name: name.to_string(),
            address: address.to_string(),
            tenant_count: 0,
            created_at: now,
        })
    }

    pub async fn list_servers(&self) -> Result<Vec<Server>> {
        let rows = sqlx::query_as::<_, (String, String, String, String)>(
            r#"
            SELECT s.id, s.name, s.address, s.created_at
            FROM servers s
            ORDER BY s.created_at
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut servers = Vec::new();
        for (id, name, address, created_at) in rows {
            let count: (i32,) = sqlx::query_as("SELECT COUNT(*) FROM tenants WHERE server_id = ?")
                .bind(&id)
                .fetch_one(&self.pool)
                .await?;

            servers.push(Server {
                id,
                name,
                address,
                tenant_count: count.0,
                created_at,
            });
        }

        Ok(servers)
    }

    pub async fn get_server(&self, id_or_name: &str) -> Result<Option<Server>> {
        let row = sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT id, name, address, created_at FROM servers WHERE id = ? OR name = ?",
        )
        .bind(id_or_name)
        .bind(id_or_name)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some((id, name, address, created_at)) => {
                let count: (i32,) =
                    sqlx::query_as("SELECT COUNT(*) FROM tenants WHERE server_id = ?")
                        .bind(&id)
                        .fetch_one(&self.pool)
                        .await?;

                Ok(Some(Server {
                    id,
                    name,
                    address,
                    tenant_count: count.0,
                    created_at,
                }))
            }
            None => Ok(None),
        }
    }

    pub async fn remove_server(&self, id_or_name: &str) -> Result<()> {
        let server = self
            .get_server(id_or_name)
            .await?
            .ok_or_else(|| anyhow!("Server not found: {}", id_or_name))?;

        if server.tenant_count > 0 {
            return Err(anyhow!(
                "Cannot remove server with {} tenants. Move or remove tenants first.",
                server.tenant_count
            ));
        }

        sqlx::query("DELETE FROM servers WHERE id = ?")
            .bind(&server.id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Tenant operations

    pub async fn add_tenant(
        &self,
        id: &str,
        server_id_or_name: Option<&str>,
        config: Option<&str>,
    ) -> Result<Tenant> {
        // Find server (specified or pick one with least tenants)
        let server = match server_id_or_name {
            Some(s) => self
                .get_server(s)
                .await?
                .ok_or_else(|| anyhow!("Server not found: {}", s))?,
            None => {
                // Pick server with least tenants
                let servers = self.list_servers().await?;
                servers
                    .into_iter()
                    .min_by_key(|s| s.tenant_count)
                    .ok_or_else(|| anyhow!("No servers available. Add a server first."))?
            }
        };

        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO tenants (id, server_id, config, status, created_at) VALUES (?, ?, ?, 'active', ?)",
        )
        .bind(id)
        .bind(&server.id)
        .bind(config)
        .bind(&now)
        .execute(&self.pool)
        .await?;

        Ok(Tenant {
            id: id.to_string(),
            server_id: server.id,
            config: config.map(|s| s.to_string()),
            status: "active".to_string(),
            created_at: now,
        })
    }

    pub async fn list_tenants(&self) -> Result<Vec<Tenant>> {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
            "SELECT id, server_id, config, status, created_at FROM tenants ORDER BY created_at",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(id, server_id, config, status, created_at)| Tenant {
                id,
                server_id,
                config,
                status,
                created_at,
            })
            .collect())
    }

    pub async fn get_tenant(&self, id: &str) -> Result<Option<Tenant>> {
        let row = sqlx::query_as::<_, (String, String, Option<String>, String, String)>(
            "SELECT id, server_id, config, status, created_at FROM tenants WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(id, server_id, config, status, created_at)| Tenant {
            id,
            server_id,
            config,
            status,
            created_at,
        }))
    }

    pub async fn remove_tenant(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM domain_aliases WHERE tenant_id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM tenants WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    // Routing lookup

    pub async fn lookup_tenant(&self, tenant_id: &str) -> Result<Option<(Tenant, Server)>> {
        let tenant = match self.get_tenant(tenant_id).await? {
            Some(t) => t,
            None => return Ok(None),
        };

        let server = self
            .get_server(&tenant.server_id)
            .await?
            .ok_or_else(|| anyhow!("Server not found for tenant: {}", tenant_id))?;

        Ok(Some((tenant, server)))
    }

    pub async fn lookup_by_domain(&self, domain: &str) -> Result<Option<String>> {
        let row = sqlx::query_as::<_, (String,)>(
            "SELECT tenant_id FROM domain_aliases WHERE domain = ?",
        )
        .bind(domain)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|(tenant_id,)| tenant_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn test_db() -> Database {
        let path = format!("/tmp/slum-test-{}.db", uuid::Uuid::new_v4());
        Database::open(&path).await.unwrap()
    }

    #[tokio::test]
    async fn test_add_and_list_servers() {
        let db = test_db().await;

        // Initially empty
        let servers = db.list_servers().await.unwrap();
        assert!(servers.is_empty());

        // Add servers
        let s1 = db.add_server("server-1", "10.0.0.1:9000").await.unwrap();
        let s2 = db.add_server("server-2", "10.0.0.2:9000").await.unwrap();

        assert_eq!(s1.name, "server-1");
        assert_eq!(s2.address, "10.0.0.2:9000");

        // List servers
        let servers = db.list_servers().await.unwrap();
        assert_eq!(servers.len(), 2);
    }

    #[tokio::test]
    async fn test_get_server_by_name_and_id() {
        let db = test_db().await;

        let server = db.add_server("my-server", "10.0.0.1:9000").await.unwrap();

        // Get by name
        let found = db.get_server("my-server").await.unwrap().unwrap();
        assert_eq!(found.id, server.id);

        // Get by ID
        let found = db.get_server(&server.id).await.unwrap().unwrap();
        assert_eq!(found.name, "my-server");

        // Not found
        let not_found = db.get_server("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn test_remove_server() {
        let db = test_db().await;

        db.add_server("server-1", "10.0.0.1:9000").await.unwrap();

        // Can remove empty server
        db.remove_server("server-1").await.unwrap();

        let servers = db.list_servers().await.unwrap();
        assert!(servers.is_empty());
    }

    #[tokio::test]
    async fn test_cannot_remove_server_with_tenants() {
        let db = test_db().await;

        db.add_server("server-1", "10.0.0.1:9000").await.unwrap();
        db.add_tenant("tenant-1", Some("server-1"), None).await.unwrap();

        // Cannot remove server with tenants
        let result = db.remove_server("server-1").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot remove server"));
    }

    #[tokio::test]
    async fn test_add_tenant_to_specific_server() {
        let db = test_db().await;

        db.add_server("server-1", "10.0.0.1:9000").await.unwrap();
        let s2 = db.add_server("server-2", "10.0.0.2:9000").await.unwrap();

        // Add to specific server
        let tenant = db.add_tenant("romneys", Some("server-2"), None).await.unwrap();
        assert_eq!(tenant.server_id, s2.id);
    }

    #[tokio::test]
    async fn test_add_tenant_auto_balance() {
        let db = test_db().await;

        db.add_server("server-1", "10.0.0.1:9000").await.unwrap();
        db.add_server("server-2", "10.0.0.2:9000").await.unwrap();

        // Add tenants without specifying server - should balance
        db.add_tenant("tenant-1", None, None).await.unwrap();
        db.add_tenant("tenant-2", None, None).await.unwrap();
        db.add_tenant("tenant-3", None, None).await.unwrap();

        let servers = db.list_servers().await.unwrap();
        // Should be roughly balanced (not all on one server)
        let counts: Vec<i32> = servers.iter().map(|s| s.tenant_count).collect();
        assert!(counts.iter().all(|&c| c >= 1)); // Each has at least 1
    }

    #[tokio::test]
    async fn test_add_tenant_no_servers() {
        let db = test_db().await;

        // No servers - should fail
        let result = db.add_tenant("tenant-1", None, None).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No servers available"));
    }

    #[tokio::test]
    async fn test_tenant_crud() {
        let db = test_db().await;

        db.add_server("server-1", "10.0.0.1:9000").await.unwrap();

        // Create
        let tenant = db.add_tenant("romneys", None, Some(r#"{"key":"value"}"#)).await.unwrap();
        assert_eq!(tenant.id, "romneys");
        assert_eq!(tenant.status, "active");

        // Read
        let found = db.get_tenant("romneys").await.unwrap().unwrap();
        assert_eq!(found.config.as_deref(), Some(r#"{"key":"value"}"#));

        // List
        let tenants = db.list_tenants().await.unwrap();
        assert_eq!(tenants.len(), 1);

        // Delete
        db.remove_tenant("romneys").await.unwrap();
        let tenants = db.list_tenants().await.unwrap();
        assert!(tenants.is_empty());
    }

    #[tokio::test]
    async fn test_lookup_tenant() {
        let db = test_db().await;

        let server = db.add_server("server-1", "10.0.0.1:9000").await.unwrap();
        db.add_tenant("romneys", None, None).await.unwrap();

        // Lookup
        let (tenant, srv) = db.lookup_tenant("romneys").await.unwrap().unwrap();
        assert_eq!(tenant.id, "romneys");
        assert_eq!(srv.id, server.id);

        // Not found
        let not_found = db.lookup_tenant("nonexistent").await.unwrap();
        assert!(not_found.is_none());
    }
}
