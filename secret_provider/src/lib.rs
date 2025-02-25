pub mod error;
pub mod file;
pub mod k8s;

use error::Error;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

/// A trait for loading encryption parameters from a secret source.
///
/// Implementors can load from various backends (file, Kubernetes API, etc.).
#[async_trait]
pub trait SecretProvider {
    async fn secret_data<T>(&self, identifier: &str) -> Result<T, Error>
    where
        T: DeserializeOwned + Send;
}
