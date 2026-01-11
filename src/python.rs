//! Python bindings for slum

use pyo3::prelude::*;
use pyo3::exceptions::PyRuntimeError;
use std::sync::Arc;
use tokio::runtime::Runtime;

use crate::db;

/// Python wrapper for the slum Database
#[pyclass]
pub struct SlumDB {
    db: Arc<db::Database>,
    runtime: Arc<Runtime>,
}

/// Server information
#[pyclass]
#[derive(Clone)]
pub struct PyServer {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub address: String,
    #[pyo3(get)]
    pub tenant_count: i32,
    #[pyo3(get)]
    pub created_at: String,
}

/// Tenant information
#[pyclass]
#[derive(Clone)]
pub struct PyTenant {
    #[pyo3(get)]
    pub id: String,
    #[pyo3(get)]
    pub server_id: String,
    #[pyo3(get)]
    pub config: Option<String>,
    #[pyo3(get)]
    pub status: String,
    #[pyo3(get)]
    pub created_at: String,
}

impl From<db::Server> for PyServer {
    fn from(s: db::Server) -> Self {
        PyServer {
            id: s.id,
            name: s.name,
            address: s.address,
            tenant_count: s.tenant_count,
            created_at: s.created_at,
        }
    }
}

impl From<db::Tenant> for PyTenant {
    fn from(t: db::Tenant) -> Self {
        PyTenant {
            id: t.id,
            server_id: t.server_id,
            config: t.config,
            status: t.status,
            created_at: t.created_at,
        }
    }
}

#[pymethods]
impl SlumDB {
    /// Open a slum database at the given path
    #[new]
    fn new(path: &str) -> PyResult<Self> {
        let runtime = Runtime::new()
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create runtime: {}", e)))?;
        
        let db = runtime.block_on(async {
            db::Database::open(path).await
        }).map_err(|e| PyRuntimeError::new_err(format!("Failed to open database: {}", e)))?;
        
        Ok(SlumDB {
            db: Arc::new(db),
            runtime: Arc::new(runtime),
        })
    }

    // Server operations

    /// Add a server to the fleet
    fn add_server(&self, name: &str, address: &str) -> PyResult<PyServer> {
        let db = self.db.clone();
        let name = name.to_string();
        let address = address.to_string();
        
        self.runtime.block_on(async move {
            db.add_server(&name, &address).await
        })
        .map(PyServer::from)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to add server: {}", e)))
    }

    /// List all servers in the fleet
    fn list_servers(&self) -> PyResult<Vec<PyServer>> {
        let db = self.db.clone();
        
        self.runtime.block_on(async move {
            db.list_servers().await
        })
        .map(|servers| servers.into_iter().map(PyServer::from).collect())
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to list servers: {}", e)))
    }

    /// Get a server by ID or name
    fn get_server(&self, id_or_name: &str) -> PyResult<Option<PyServer>> {
        let db = self.db.clone();
        let id_or_name = id_or_name.to_string();
        
        self.runtime.block_on(async move {
            db.get_server(&id_or_name).await
        })
        .map(|opt| opt.map(PyServer::from))
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to get server: {}", e)))
    }

    /// Remove a server from the fleet
    fn remove_server(&self, id_or_name: &str) -> PyResult<()> {
        let db = self.db.clone();
        let id_or_name = id_or_name.to_string();
        
        self.runtime.block_on(async move {
            db.remove_server(&id_or_name).await
        })
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to remove server: {}", e)))
    }

    // Tenant operations

    /// Add a tenant to the fleet
    #[pyo3(signature = (id, server=None, config=None))]
    fn add_tenant(&self, id: &str, server: Option<&str>, config: Option<&str>) -> PyResult<PyTenant> {
        let db = self.db.clone();
        let id = id.to_string();
        let server = server.map(|s| s.to_string());
        let config = config.map(|s| s.to_string());
        
        self.runtime.block_on(async move {
            db.add_tenant(&id, server.as_deref(), config.as_deref()).await
        })
        .map(PyTenant::from)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to add tenant: {}", e)))
    }

    /// List all tenants
    fn list_tenants(&self) -> PyResult<Vec<PyTenant>> {
        let db = self.db.clone();
        
        self.runtime.block_on(async move {
            db.list_tenants().await
        })
        .map(|tenants| tenants.into_iter().map(PyTenant::from).collect())
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to list tenants: {}", e)))
    }

    /// Get a tenant by ID
    fn get_tenant(&self, id: &str) -> PyResult<Option<PyTenant>> {
        let db = self.db.clone();
        let id = id.to_string();
        
        self.runtime.block_on(async move {
            db.get_tenant(&id).await
        })
        .map(|opt| opt.map(PyTenant::from))
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to get tenant: {}", e)))
    }

    /// Remove a tenant
    fn remove_tenant(&self, id: &str) -> PyResult<()> {
        let db = self.db.clone();
        let id = id.to_string();
        
        self.runtime.block_on(async move {
            db.remove_tenant(&id).await
        })
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to remove tenant: {}", e)))
    }

    // Routing operations

    /// Lookup tenant and server for routing
    fn lookup_tenant(&self, tenant_id: &str) -> PyResult<Option<(PyTenant, PyServer)>> {
        let db = self.db.clone();
        let tenant_id = tenant_id.to_string();
        
        self.runtime.block_on(async move {
            db.lookup_tenant(&tenant_id).await
        })
        .map(|opt| opt.map(|(t, s)| (PyTenant::from(t), PyServer::from(s))))
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to lookup tenant: {}", e)))
    }

    /// Lookup tenant ID by domain alias
    fn lookup_by_domain(&self, domain: &str) -> PyResult<Option<String>> {
        let db = self.db.clone();
        let domain = domain.to_string();
        
        self.runtime.block_on(async move {
            db.lookup_by_domain(&domain).await
        })
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to lookup domain: {}", e)))
    }
}

/// Python module
#[pymodule]
fn slum(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SlumDB>()?;
    m.add_class::<PyServer>()?;
    m.add_class::<PyTenant>()?;
    Ok(())
}
