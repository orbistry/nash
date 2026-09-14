//! File source abstraction for runtime-agnostic file I/O.
//!
//! The `FileSource` trait provides an async interface for reading and writing files,
//! enabling the driver to work with different backends:
//! - Native filesystem (for CLI)
//! - In-memory overlays (for LSP unsaved buffers)
//! - HTTP fetch (for WASM playground)

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use url::Url;

use crate::error::DriverError;

/// Async file source abstraction.
///
/// All file operations use URLs for portability across platforms.
/// File URLs have the scheme `file://`.
#[async_trait]
pub trait FileSource: Send + Sync {
    /// Check if a file exists.
    async fn exists(&self, uri: &Url) -> Result<bool, DriverError>;

    /// Read file contents as a string.
    async fn read(&self, uri: &Url) -> Result<String, DriverError>;

    /// Write file contents.
    async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError>;

    /// List files matching a glob pattern in a directory.
    ///
    /// Returns URLs for all matching files.
    async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError>;
}

// =============================================================================
// FileSystemSource - Native filesystem implementation
// =============================================================================

/// File source backed by the native filesystem.
#[cfg(not(target_arch = "wasm32"))]
pub struct FileSystemSource;

#[cfg(not(target_arch = "wasm32"))]
impl FileSystemSource {
    pub fn new() -> Self {
        FileSystemSource
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for FileSystemSource {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[async_trait]
impl FileSource for FileSystemSource {
    async fn exists(&self, uri: &Url) -> Result<bool, DriverError> {
        let path = uri_to_path(uri)?;
        Ok(tokio::fs::try_exists(&path).await.unwrap_or(false))
    }

    async fn read(&self, uri: &Url) -> Result<String, DriverError> {
        let path = uri_to_path(uri)?;
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|source| DriverError::ReadError { path, source })
    }

    async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError> {
        let path = uri_to_path(uri)?;

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|source| DriverError::WriteError {
                    path: parent.to_path_buf(),
                    source,
                })?;
        }

        tokio::fs::write(&path, content)
            .await
            .map_err(|source| DriverError::WriteError { path, source })
    }

    async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError> {
        let base_path = uri_to_path(base)?;
        let full_pattern = base_path.join(pattern);

        let pattern_str = full_pattern.to_string_lossy();
        let mut urls = Vec::new();

        // Use glob crate for pattern matching
        for entry in glob::glob(&pattern_str).map_err(|e| DriverError::InvalidModulePath {
            path: std::path::PathBuf::from(e.msg),
        })? {
            match entry {
                Ok(path) => {
                    if let Ok(url) = Url::from_file_path(&path) {
                        urls.push(url);
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(urls)
    }
}

// =============================================================================
// InMemorySource - HashMap-based implementation for testing/LSP
// =============================================================================

/// In-memory file source backed by a HashMap.
///
/// Useful for:
/// - Testing without touching the filesystem
/// - LSP unsaved buffers (overlay on top of filesystem)
pub struct InMemorySource {
    files: RwLock<HashMap<Url, String>>,
}

impl InMemorySource {
    pub fn new() -> Self {
        InMemorySource {
            files: RwLock::new(HashMap::new()),
        }
    }

    /// Create from a collection of (uri, content) pairs.
    pub fn with_files(files: impl IntoIterator<Item = (Url, String)>) -> Self {
        InMemorySource {
            files: RwLock::new(files.into_iter().collect()),
        }
    }

    /// Insert a file into memory.
    pub fn insert(&self, uri: Url, content: String) {
        self.files.write().insert(uri, content);
    }

    /// Remove a file from memory.
    pub fn remove(&self, uri: &Url) -> Option<String> {
        self.files.write().remove(uri)
    }
}

impl Default for InMemorySource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FileSource for InMemorySource {
    async fn exists(&self, uri: &Url) -> Result<bool, DriverError> {
        Ok(self.files.read().contains_key(uri))
    }

    async fn read(&self, uri: &Url) -> Result<String, DriverError> {
        self.files
            .read()
            .get(uri)
            .cloned()
            .ok_or_else(|| DriverError::FileNotFound { uri: uri.clone() })
    }

    async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError> {
        self.files.write().insert(uri.clone(), content.to_string());
        Ok(())
    }

    async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError> {
        let matcher =
            glob::Pattern::new(pattern).map_err(|error| DriverError::InvalidModulePath {
                path: error.msg.into(),
            })?;
        let base_path = base.path().trim_end_matches('/');
        let files = self.files.read();
        let mut urls: Vec<Url> = files
            .keys()
            .filter(|uri| {
                if uri.scheme() != base.scheme() || uri.authority() != base.authority() {
                    return false;
                }
                let Some(relative) = uri
                    .path()
                    .strip_prefix(base_path)
                    .and_then(|path| path.strip_prefix('/'))
                else {
                    return false;
                };
                matcher.matches_with(
                    relative,
                    glob::MatchOptions {
                        require_literal_separator: true,
                        ..glob::MatchOptions::new()
                    },
                )
            })
            .cloned()
            .collect();
        urls.sort();
        Ok(urls)
    }
}

// =============================================================================
// OverlaySource - Layered file source
// =============================================================================

/// Overlay file source that tries a primary source first, then falls back.
///
/// Useful for LSP: overlay unsaved buffers on top of the filesystem.
pub struct OverlaySource<P: FileSource, F: FileSource> {
    primary: P,
    fallback: F,
}

impl<P: FileSource, F: FileSource> OverlaySource<P, F> {
    pub fn new(primary: P, fallback: F) -> Self {
        OverlaySource { primary, fallback }
    }
}

#[async_trait]
impl<P: FileSource, F: FileSource> FileSource for OverlaySource<P, F> {
    async fn exists(&self, uri: &Url) -> Result<bool, DriverError> {
        if self.primary.exists(uri).await? {
            return Ok(true);
        }
        self.fallback.exists(uri).await
    }

    async fn read(&self, uri: &Url) -> Result<String, DriverError> {
        match self.primary.read(uri).await {
            Ok(content) => Ok(content),
            Err(DriverError::FileNotFound { .. }) => self.fallback.read(uri).await,
            Err(e) => Err(e),
        }
    }

    async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError> {
        // Always write to primary
        self.primary.write(uri, content).await
    }

    async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError> {
        // Combine results from both sources, deduplicating
        let mut urls: Vec<Url> = self.fallback.glob(base, pattern).await?;
        let primary_urls = self.primary.glob(base, pattern).await?;

        for url in primary_urls {
            if !urls.contains(&url) {
                urls.push(url);
            }
        }

        Ok(urls)
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Convert a file URL to a filesystem path.
#[cfg(not(target_arch = "wasm32"))]
fn uri_to_path(uri: &Url) -> Result<std::path::PathBuf, DriverError> {
    uri.to_file_path()
        .map_err(|()| DriverError::InvalidFileUri { uri: uri.clone() })
}

/// Convert a filesystem path to a file URL.
pub fn path_to_uri(path: &Path) -> Result<Url, DriverError> {
    Url::from_file_path(path).map_err(|()| DriverError::InvalidModulePath {
        path: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn memory_globs_match_registered_extensions_with_directory_boundaries() {
        let source = InMemorySource::with_files(
            [
                ("file:///project/src/main.ak", ""),
                ("file:///project/src/folder/math.ak", ""),
                ("file:///project/src/Main.nash", ""),
                ("file:///project/src/notes.txt", ""),
                ("file:///project/src-other/hidden.ak", ""),
            ]
            .map(|(uri, source)| (Url::parse(uri).unwrap(), source.to_owned())),
        );
        let root = Url::parse("file:///project/src").unwrap();
        assert_eq!(
            source.glob(&root, "**/*.ak").await.unwrap(),
            [
                Url::parse("file:///project/src/folder/math.ak").unwrap(),
                Url::parse("file:///project/src/main.ak").unwrap(),
            ],
        );
        assert_eq!(
            source.glob(&root, "*.ak").await.unwrap(),
            [Url::parse("file:///project/src/main.ak").unwrap()],
        );
        assert_eq!(
            source.glob(&root, "**/*.nash").await.unwrap(),
            [Url::parse("file:///project/src/Main.nash").unwrap()],
        );
    }

    #[tokio::test]
    async fn test_in_memory_source() {
        let source = InMemorySource::new();
        let uri = Url::parse("file:///test/Main.nash").unwrap();

        // Initially not found
        assert!(!source.exists(&uri).await.unwrap());
        assert!(source.read(&uri).await.is_err());

        // Write and read back
        source
            .write(&uri, "module Main exposing (..)")
            .await
            .unwrap();
        assert!(source.exists(&uri).await.unwrap());
        assert_eq!(
            source.read(&uri).await.unwrap(),
            "module Main exposing (..)"
        );

        // Remove
        source.remove(&uri);
        assert!(!source.exists(&uri).await.unwrap());
    }

    #[tokio::test]
    async fn test_overlay_source() {
        let primary = InMemorySource::new();
        let fallback = InMemorySource::new();

        let uri = Url::parse("file:///test/Main.nash").unwrap();

        // Put content in fallback
        fallback.write(&uri, "fallback content").await.unwrap();

        let overlay = OverlaySource::new(primary, fallback);

        // Should read from fallback
        assert_eq!(overlay.read(&uri).await.unwrap(), "fallback content");

        // Write to overlay (goes to primary)
        overlay.write(&uri, "primary content").await.unwrap();

        // Now should read from primary
        assert_eq!(overlay.read(&uri).await.unwrap(), "primary content");
    }
}
