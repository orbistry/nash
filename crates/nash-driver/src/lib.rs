//! Nash compiler driver and build system.
//!
//! This crate provides the infrastructure for building Nash projects:
//!
//! - **File abstraction**: `FileSource` trait for runtime-agnostic file I/O
//! - **Caching**: `Database` for managing source files and compilation results
//! - **Project loading**: Parse `nash.jsonc` and discover source files
//! - **Dependency graph**: Build and analyze module dependencies
//! - **Compilation**: Fetch sources concurrently, then compile in dependency order
//! - **Incremental builds**: Interface-based caching for fast rebuilds
//!
//! # Example
//!
//! ```ignore
//! use nash_driver::{Project, Database, build, build_graph};
//! use nash_driver::source::FileSystemSource;
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), nash_driver::DriverError> {
//!     // Load project
//!     let project = Project::load(".").await?;
//!
//!     // Create database with filesystem source
//!     let db = Arc::new(Mutex::new(Database::new(FileSystemSource::new())));
//!
//!     // Discover modules
//!     let modules = project.discover_modules(&db.lock().await).await?;
//!
//!     // Build dependency graph
//!     let graph = build_graph(db.clone(), &modules.keys().cloned().collect::<Vec<_>>()).await?;
//!
//!     // Compile everything
//!     let result = build(db, &graph, &modules).await;
//!
//!     println!("Compiled {} modules ({} success, {} failed)",
//!         result.total, result.success, result.failed);
//!
//!     Ok(())
//! }
//! ```

pub mod compile;
pub mod database;
pub mod error;
pub mod graph;
pub mod interface;
pub mod project;
pub mod source;

// Re-export main types
pub use compile::{
    BuildResult, ModuleResult, Solved, SolvedModule, build, build_graph, build_with,
};
pub use database::Database;
pub use error::DriverError;
pub use graph::DepGraph;
pub use interface::{Export, Interface};
pub use project::{ModuleOrigins, Project, ProjectMember};
pub use source::{FileSource, FileSystemSource, InMemorySource, OverlaySource};
