//! Typed client for the public API contract.
//!
//! - [`cli`] contains clap subcommands for each resource group.
//! - [`generated`] contains the committed transport and operations.
//! - [`commands`] dispatches commands through the client.
//! - [`error`] contains the public response and error model.

pub mod cli;
pub mod commands;
pub mod error;
pub mod generated;

pub use cli::V1Command;
pub use generated::Client;
