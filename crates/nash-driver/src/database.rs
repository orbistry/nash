//! Compilation database with caching.
//!
//! The `Database` manages file sources and caches compilation results
//! to enable incremental compilation.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use url::Url;

use crate::error::DriverError;
use crate::source::FileSource;

/// Compilation database managing source files and cached results.
///
/// The database provides:
/// - File reading via a `FileSource` abstraction
/// - Caching of source text
pub struct Database {
    /// File source for reading/writing files.
    source: Arc<dyn FileSource>,

    /// Cached source text keyed by URI.
    files: HashMap<Url, String>,
}

impl Database {
    /// Create a new database with the given file source.
    pub fn new(source: impl FileSource + 'static) -> Self {
        Database {
            source: Arc::new(crate::bundled_base::BundledSource(source)),
            files: HashMap::new(),
        }
    }

    /// Get the source text for a file, reading from cache or disk.
    pub async fn source(&mut self, uri: &Url) -> Result<&str, DriverError> {
        // If not cached, read from source
        if !self.files.contains_key(uri) {
            let content = self.source.read(uri).await?;
            self.files.insert(uri.clone(), content);
        }

        Ok(self.files.get(uri).unwrap())
    }

    /// Source text for each URI, in order; uncached files are read concurrently outside the lock.
    pub(crate) async fn sources(
        db: &Mutex<Self>,
        uris: &[&Url],
    ) -> Vec<Result<String, DriverError>> {
        let reads: Vec<_> = {
            let db = db.lock().await;
            uris.iter()
                .map(|&uri| {
                    let cached = db.files.get(uri).cloned();
                    let source = db.source.clone();
                    let uri = uri.clone();
                    tokio::spawn(async move {
                        match cached {
                            Some(text) => Ok(text),
                            None => source.read(&uri).await,
                        }
                    })
                })
                .collect()
        };
        let mut sources = Vec::with_capacity(reads.len());
        for read in reads {
            sources.push(read.await.expect("source read panicked"));
        }
        let mut db = db.lock().await;
        for (&uri, source) in uris.iter().zip(&sources) {
            if let Ok(text) = source {
                db.files.entry(uri.clone()).or_insert_with(|| text.clone());
            }
        }
        sources
    }

    /// Check if a file exists.
    pub async fn exists(&self, uri: &Url) -> Result<bool, DriverError> {
        self.source.exists(uri).await
    }

    /// List files matching a glob pattern.
    pub async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError> {
        self.source.glob(base, pattern).await
    }

    /// Write content to a file.
    pub async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError> {
        self.source.write(uri, content).await
    }

    /// Invalidate a file's cached content.
    ///
    /// This removes the file from the cache, forcing a re-read on next access.
    pub fn invalidate(&mut self, uri: &Url) {
        self.files.remove(uri);
    }

    /// Get the underlying file source (for operations that bypass caching).
    pub fn file_source(&self) -> &dyn FileSource {
        self.source.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::InMemorySource;

    #[tokio::test]
    async fn test_database_invalidation() {
        let mem = InMemorySource::new();
        let uri = Url::parse("file:///test/Main.nash").unwrap();
        mem.insert(uri.clone(), "original".to_string());

        let mut db = Database::new(mem);

        assert_eq!(db.source(&uri).await.unwrap(), "original");
        db.write(&uri, "updated").await.unwrap();
        assert_eq!(db.source(&uri).await.unwrap(), "original");

        db.invalidate(&uri);
        assert_eq!(db.source(&uri).await.unwrap(), "updated");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn sources_preserve_order_and_errors() {
        let mem = InMemorySource::new();
        let uris: Vec<_> = (0..128)
            .map(|i| Url::parse(&format!("file:///Module{i}.nash")).unwrap())
            .collect();
        for (index, uri) in uris.iter().enumerate() {
            if index != 63 {
                mem.insert(uri.clone(), format!("source {index}"));
            }
        }
        let db = Mutex::new(Database::new(mem));
        let sources = Database::sources(&db, &uris.iter().collect::<Vec<_>>()).await;
        assert_eq!(sources.len(), uris.len());
        for (index, source) in sources.iter().enumerate() {
            if index == 63 {
                assert!(source.is_err(), "missing source must retain its position");
            } else {
                assert_eq!(source.as_ref().unwrap(), &format!("source {index}"));
            }
        }
    }

    /// Each read waits until every read has started.
    struct Rendezvous(tokio::sync::Barrier);

    #[async_trait::async_trait]
    impl FileSource for Rendezvous {
        async fn exists(&self, _: &Url) -> Result<bool, DriverError> {
            Ok(true)
        }
        async fn read(&self, uri: &Url) -> Result<String, DriverError> {
            self.0.wait().await;
            Ok(uri.path().to_owned())
        }
        async fn write(&self, _: &Url, _: &str) -> Result<(), DriverError> {
            unreachable!()
        }
        async fn glob(&self, _: &Url, _: &str) -> Result<Vec<Url>, DriverError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn sources_read_concurrently_and_fill_the_cache() {
        let uris: Vec<_> = (0..4)
            .map(|i| Url::parse(&format!("file:///M{i}.nash")).unwrap())
            .collect();
        let db = Mutex::new(Database::new(Rendezvous(tokio::sync::Barrier::new(
            uris.len(),
        ))));
        let fetch = async {
            let sources = Database::sources(&db, &uris.iter().collect::<Vec<_>>()).await;
            // A second read would wait at the barrier forever.
            let cached = db.lock().await.source(&uris[0]).await.unwrap().to_owned();
            (sources, cached)
        };
        let (sources, cached) = tokio::time::timeout(std::time::Duration::from_secs(5), fetch)
            .await
            .expect("reads must run concurrently and results must be cached");
        let paths: Vec<_> = sources.into_iter().map(Result::unwrap).collect();
        assert_eq!(paths, ["/M0.nash", "/M1.nash", "/M2.nash", "/M3.nash"]);
        assert_eq!(cached, "/M0.nash");
    }
}
