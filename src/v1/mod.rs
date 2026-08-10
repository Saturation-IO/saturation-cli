//! The `sat /v1` namespace: a typed client generated from the same OpenAPI 3.1
//! that drives the TS SDK, the Scalar docs and the contract tests.
//!
//! - [`cli`] — clap subcommands for every `/v1` resource group.
//! - [`generated`] — the generated transport + operations (progenitor target;
//!   see `build.rs`).
//! - [`commands`] — the thin ergonomic dispatch layer over the generated client.
//! - [`error`] — the §5d response/error model.

pub mod cli;
pub mod commands;
pub mod error;
pub mod generated;

pub use cli::V1Args;
pub use generated::Client;
