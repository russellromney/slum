//! Slum - Fleet orchestrator for tenement servers
//!
//! This library provides Python bindings for managing a fleet of tenement servers.
//! Use it to add/remove servers, manage tenants, and lookup routing information.

pub mod db;

#[cfg(feature = "python")]
mod python;

#[cfg(feature = "python")]
pub use python::*;

// Re-export main types for Rust users
pub use db::{Database, Server, Tenant};
