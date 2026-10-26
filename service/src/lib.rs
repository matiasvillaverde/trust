//! Authenticated application service. Transports delegate to the same serialized worker.

pub mod auth;
pub mod catalog;
pub mod config;
pub mod domain;
pub mod error;
pub mod http;
pub mod journal;
pub mod mcp;
pub mod runtime;

pub mod skills;
