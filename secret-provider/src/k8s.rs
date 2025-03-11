//! This module reads from a secret provided the given key matches. Consider the example as below.
//!
//!```yaml
//! apiVersion: v1
//! kind: Secret
//! metadata:
//!   name: my-secret
//!   namespace: default
//! type: Opaque
//! stringData:
//!   encryption_parameters: |
//!    {
//!       "cipher": "AesCbc",
//!       "key": {
//!        "key_name": "example-key",
//!        "key": "2b7e151628aed2a6abf7158809cf4f3c",
//!         "key_length": 128
//!       }
//!     }
//! ```
//!
use crate::SecretProvider;

use crate::error::Error;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Secret;
use serde::de::DeserializeOwned;

const SECRET_PARAMS_KEY: &str = "encryption_parameters";

/// A secret provider that retrieves a Kubernetes secret.
pub struct K8sSecretProvider {
    /// The Kubernetes client.
    client: kube::Client,
    /// The namespace in which to look for the secret.
    namespace: String,
    /// The key within the secret’s data containing the encryption parameters.
    secret_params_key: String,
}

impl K8sSecretProvider {
    /// Create a new `K8sSecretProvider` using the provided Kubernetes client and namespace.
    pub fn new(client: kube::Client, namespace: impl Into<String>) -> Self {
        Self {
            client,
            namespace: namespace.into(),
            secret_params_key: SECRET_PARAMS_KEY.to_string(),
        }
    }
}

#[async_trait]
impl SecretProvider for K8sSecretProvider {
    async fn secret_data<T>(&self, secret_name: &str) -> Result<T, Error>
    where
        T: DeserializeOwned + Send,
    {
        use kube::api::Api;
        let secrets: Api<Secret> = Api::namespaced(self.client.clone(), &self.namespace);
        let secret = secrets
            .get(secret_name)
            .await
            .map_err(|e| Error::KubeApi { source: e })?;

        let data = secret.data.ok_or_else(|| Error::MissingSecretData {})?;
        let raw_bytes = &data
            .get(&self.secret_params_key)
            .ok_or_else(|| Error::MissingSecretKey {
                key: self.secret_params_key.clone(),
            })?
            .0;
        let raw_str = std::str::from_utf8(raw_bytes)
            .map_err(|error| Error::Utf8Conversion { source: error })?;
        let secret: T =
            serde_json::from_str(raw_str).map_err(|error| Error::ParseJson { source: error })?;
        Ok(secret)
    }
}
