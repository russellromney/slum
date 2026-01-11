# slum

**Fleet orchestrator for tenement servers.**

slum routes requests to the correct tenement server based on subdomain, manages the tenant→server registry, and provides an API for programmatic control.

## Installation

```bash
cargo install slum
```

## Quick Start

```bash
# Add tenement servers to your fleet
slum server-add 10.0.0.1:9000 -n tenement-1
slum server-add 10.0.0.2:9000 -n tenement-2

# Add tenants (auto-balances across servers)
slum tenant-add romneys
slum tenant-add smiths
slum tenant-add jones

# Check status
slum status

# Start the proxy server
slum serve -p 8080
```

## How It Works

```
Request: romneys.ourfam.lol/api/events
              │
              ▼
┌─────────────────────────────┐
│ slum (reverse proxy)        │
│ - Extract tenant: "romneys" │
│ - Lookup: romneys → server1 │
│ - Proxy to server1          │
└─────────────────────────────┘
              │
              ▼
┌─────────────────────────────┐
│ tenement-1 (10.0.0.1:9000)  │
│ - Receives request          │
│ - Spawns/routes to process  │
└─────────────────────────────┘
```

## CLI Commands

```bash
# Server management
slum server-add <address> [-n name]    # Add a tenement server
slum server-list                        # List all servers
slum server-remove <id-or-name>         # Remove a server

# Tenant management
slum tenant-add <id> [-s server]        # Add tenant (auto-picks server if not specified)
slum tenant-list                        # List all tenants
slum tenant-remove <id>                 # Remove tenant

# Operations
slum serve [-p port]                    # Start proxy server
slum status                             # Fleet overview
```

## HTTP API

When running `slum serve`, these endpoints are available:

```
GET  /api/health                # Health check
GET  /api/servers               # List servers
POST /api/servers               # Add server {"name": "...", "address": "..."}
DELETE /api/servers/:id         # Remove server

GET  /api/tenants               # List tenants
POST /api/tenants               # Add tenant {"id": "...", "server": "...", "config": "..."}
DELETE /api/tenants/:id         # Remove tenant
```

All other requests are proxied to the appropriate tenement server based on the `Host` header subdomain.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ Village / Apartment / Your SaaS                                     │
│ - Signup, billing, UI                                               │
│ - Calls slum API to manage tenants                                  │
└─────────────────────────────────────────────────────────────────────┘
                              │ uses
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│ slum (this project)                                                 │
│ - Routes requests by subdomain → server                             │
│ - Registry: which tenant lives where                                │
│ - API for programmatic control                                      │
└─────────────────────────────────────────────────────────────────────┘
                              │ manages
                              ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│ tenement        │  │ tenement        │  │ tenement        │
│ (server 1)      │  │ (server 2)      │  │ (server 3)      │
│ N processes     │  │ N processes     │  │ N processes     │
└─────────────────┘  └─────────────────┘  └─────────────────┘
```

## License

Apache 2.0
