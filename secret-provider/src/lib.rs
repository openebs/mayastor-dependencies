pub mod error;
pub mod file;
pub mod k8s;

use error::Error;
use tracing::trace;

use async_trait::async_trait;
use platform::{current_platform_type, PlatformType};
use serde::de::DeserializeOwned;

/// Parse and return encryption parameters from a Kubernetes secret or a file.
pub async fn secret_data<T: DeserializeOwned + Send>(secret_source_name: &str) -> Result<T, Error> {
    let secret_params = match current_platform_type() {
        PlatformType::K8s => {
            let client = kube::Client::try_default()
                .await
                .map_err(|e| Error::KubeApi { source: e })?;
            // todo: Add cli arg for namespace as well?
            let ns = match std::env::var("MY_POD_NAMESPACE") {
                Ok(ns) if !ns.is_empty() => ns,
                _ => client.default_namespace().to_string(),
            };
            let secret_provider = k8s::K8sSecretProvider::new(client, &ns);
            trace!(
                "Platform: K8S. read secret params from 'Secret' {:?}",
                secret_source_name
            );

            let secret_params: Option<T> = secret_provider.secret_data(secret_source_name).await?;

            secret_params
        }
        // XXX: Assuming this base path for now. Need to implement capturing
        // a base path via cli or env.
        PlatformType::Deployer => {
            let file_provider = file::FileSecretProvider::new("/host/tmp".into());
            trace!(
                "Platform: Deployer. read secret params from file {:?}",
                secret_source_name
            );

            let secret_params: Option<T> = file_provider.secret_data(secret_source_name).await?;

            secret_params
        }
        PlatformType::None => None,
    };

    // Be very sure that we get the params otherwise always return error.
    secret_params.ok_or(Error::MissingSecretData {})
}

/// A trait for loading encryption parameters from a secret source.
///
/// Implementors can load from various backends (file, Kubernetes API, etc.).
#[async_trait]
pub trait SecretProvider {
    async fn secret_data<T>(&self, identifier: &str) -> Result<T, Error>
    where
        T: DeserializeOwned + Send;
}
