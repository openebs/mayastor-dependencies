use crate::{error::Error, SecretProvider};
use std::path::PathBuf;

use async_trait::async_trait;
use serde::de::DeserializeOwned;

/// A secret provider that reads a file from the provided path.
pub struct FileSecretProvider {
    base_path: PathBuf,
}

impl FileSecretProvider {
    /// Create a new `FileSecretProvider` using the provided base path.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }
}

#[async_trait]
impl SecretProvider for FileSecretProvider {
    async fn secret_data<T>(&self, identifier: &str) -> Result<T, Error>
    where
        T: DeserializeOwned + Send,
    {
        let file_path = self.base_path.join(identifier);
        let content = tokio::fs::read_to_string(file_path)
            .await
            .map_err(|error| Error::ReadFile {
                source: error,
                file: identifier.to_string(),
            })?;
        let secret =
            serde_json::from_str(&content).map_err(|error| Error::ParseJson { source: error })?;
        Ok(secret)
    }
}
