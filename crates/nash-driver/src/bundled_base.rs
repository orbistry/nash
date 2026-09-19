//! Compiler-versioned Base sources. Compilation never reads an installed source directory.
use crate::{DriverError, FileSource, ModuleOrigins};
use url::Url;

include!(concat!(env!("OUT_DIR"), "/base_sources.rs"));

pub fn uri(name: &str) -> Url {
    Url::parse(&format!("nash-base:///{}.nash", name.replace('.', "/"))).unwrap()
}

pub fn source(uri: &Url) -> Option<&'static str> {
    SOURCES
        .iter()
        .find_map(|(name, source)| (self::uri(name) == *uri).then_some(*source))
}

pub fn modules() -> ModuleOrigins {
    let package: nash_config::PackageName =
        "nash/base".parse().expect("compiler-owned package name");
    SOURCES
        .iter()
        .map(|(name, _)| (uri(name), Some(package.clone())))
        .collect()
}

pub(crate) struct BundledSource<S>(pub S);

#[async_trait::async_trait]
impl<S: FileSource> FileSource for BundledSource<S> {
    async fn exists(&self, uri: &Url) -> Result<bool, DriverError> {
        if uri.scheme() == "nash-base" {
            Ok(source(uri).is_some())
        } else {
            self.0.exists(uri).await
        }
    }
    async fn read(&self, uri: &Url) -> Result<String, DriverError> {
        match source(uri) {
            Some(text) => Ok(text.to_owned()),
            None if uri.scheme() == "nash-base" => {
                Err(DriverError::FileNotFound { uri: uri.clone() })
            }
            None => self.0.read(uri).await,
        }
    }
    async fn write(&self, uri: &Url, content: &str) -> Result<(), DriverError> {
        if uri.scheme() == "nash-base" {
            return Err(DriverError::Dependency {
                package: "nash/base".into(),
                message: "bundled compiler sources are read-only".into(),
            });
        }
        self.0.write(uri, content).await
    }
    async fn glob(&self, base: &Url, pattern: &str) -> Result<Vec<Url>, DriverError> {
        self.0.glob(base, pattern).await
    }
}
