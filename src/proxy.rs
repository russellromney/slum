use axum::{
    body::Body,
    extract::{Host, State},
    http::{Request, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use hyper_util::{client::legacy::Client, rt::TokioExecutor};

use crate::AppState;

/// Extract tenant ID from Host header
/// Examples:
///   romneys.ourfam.lol -> romneys
///   smiths.example.com -> smiths
///   localhost:8080 -> None (no subdomain)
fn extract_tenant_from_host(host: &str) -> Option<String> {
    // Remove port if present
    let host = host.split(':').next().unwrap_or(host);

    // Split by dots
    let parts: Vec<&str> = host.split('.').collect();

    // Need at least 3 parts for a subdomain (tenant.domain.tld)
    // Or 2 parts if it's tenant.localhost
    if parts.len() >= 3 {
        Some(parts[0].to_string())
    } else if parts.len() == 2 && parts[1] == "localhost" {
        Some(parts[0].to_string())
    } else {
        None
    }
}

pub async fn handle_request(
    State(state): State<AppState>,
    Host(host): Host,
    req: Request<Body>,
) -> Response {
    // Extract tenant from subdomain
    let tenant_id = match extract_tenant_from_host(&host) {
        Some(id) => id,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                "No tenant specified. Use subdomain like: tenant.yourdomain.com",
            )
                .into_response();
        }
    };

    // Look up tenant -> server mapping
    let (tenant, server) = match state.db.lookup_tenant(&tenant_id).await {
        Ok(Some(result)) => result,
        Ok(None) => {
            // Try domain alias lookup
            match state.db.lookup_by_domain(&host).await {
                Ok(Some(tid)) => match state.db.lookup_tenant(&tid).await {
                    Ok(Some(result)) => result,
                    _ => {
                        return (StatusCode::NOT_FOUND, format!("Tenant not found: {}", tenant_id))
                            .into_response()
                    }
                },
                _ => {
                    return (StatusCode::NOT_FOUND, format!("Tenant not found: {}", tenant_id))
                        .into_response()
                }
            }
        }
        Err(e) => {
            tracing::error!("Database error looking up tenant {}: {}", tenant_id, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    if tenant.status != "active" {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            format!("Tenant {} is {}", tenant_id, tenant.status),
        )
            .into_response();
    }

    // Build upstream URL
    // The tenement server handles routing to the correct process via its own proxy
    let path = req.uri().path();
    let query = req.uri().query().map(|q| format!("?{}", q)).unwrap_or_default();

    let upstream_url = format!("http://{}{}{}", server.address, path, query);

    tracing::debug!(
        "Proxying {} {} -> {}",
        tenant_id,
        req.uri().path(),
        upstream_url
    );

    // Create HTTP client and proxy the request
    let client = Client::builder(TokioExecutor::new()).build_http();

    // Build new request for upstream
    let (parts, body) = req.into_parts();

    let upstream_uri: Uri = match upstream_url.parse() {
        Ok(uri) => uri,
        Err(e) => {
            tracing::error!("Invalid upstream URL {}: {}", upstream_url, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid upstream URL").into_response();
        }
    };

    let mut upstream_req = Request::builder()
        .method(parts.method)
        .uri(upstream_uri);

    // Copy headers, adding X-Tenant-ID
    for (key, value) in parts.headers.iter() {
        // Skip host header (will be set by hyper)
        if key != "host" {
            upstream_req = upstream_req.header(key, value);
        }
    }
    upstream_req = upstream_req.header("X-Tenant-ID", &tenant_id);

    let upstream_req = match upstream_req.body(body) {
        Ok(req) => req,
        Err(e) => {
            tracing::error!("Failed to build upstream request: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to build request").into_response();
        }
    };

    // Send request to upstream
    match client.request(upstream_req).await {
        Ok(response) => {
            let (parts, body) = response.into_parts();
            Response::from_parts(parts, Body::new(body))
        }
        Err(e) => {
            tracing::error!("Upstream request failed for tenant {}: {}", tenant_id, e);
            (
                StatusCode::BAD_GATEWAY,
                format!("Failed to reach tenant server: {}", e),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tenant() {
        assert_eq!(
            extract_tenant_from_host("romneys.ourfam.lol"),
            Some("romneys".to_string())
        );
        assert_eq!(
            extract_tenant_from_host("smiths.example.com"),
            Some("smiths".to_string())
        );
        assert_eq!(
            extract_tenant_from_host("romneys.ourfam.lol:8080"),
            Some("romneys".to_string())
        );
        assert_eq!(
            extract_tenant_from_host("romneys.localhost"),
            Some("romneys".to_string())
        );
        assert_eq!(extract_tenant_from_host("localhost:8080"), None);
        assert_eq!(extract_tenant_from_host("ourfam.lol"), None);
    }
}
