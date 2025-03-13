use secret_provider::{
    error::Error, file::FileSecretProvider, k8s::K8sSecretProvider, SecretProvider,
};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
pub struct EncryptionParameters {
    /// Cipher to be used.
    pub cipher: Cipher,
    /// The encryption key.
    pub key: EncryptionKey,
}

/// The encryption key details.
#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
pub struct EncryptionKey {
    /// Name of the key.
    pub key_name: String,
    /// The AES encryption key.
    pub key: String,
    /// AES key length.
    pub key_length: u32,
    /// Secondary key (if applicable).
    pub key2: Option<String>,
    /// Length of the secondary key.
    pub key2_length: Option<u32>,
}

/// Supported cipher types.
#[derive(Serialize, Deserialize, Default, Debug, Clone, Eq, PartialEq)]
pub enum Cipher {
    #[default]
    AesCbc,
    AesXts,
}

#[tokio::main]
/// Run this example from secret-provider directory.
async fn main() -> Result<(), Error> {
    let cwd = std::env::current_dir().unwrap();
    println!("Encryption parameters file location: {cwd:?}");

    let k8s_provider = FileSecretProvider::new(cwd);
    let raw_secret: EncryptionParameters = k8s_provider
        .secret_data("example_secret_params.json")
        .await?;
    println!("Encryption Parameters from file: {:?}", raw_secret);

    let client = kube::Client::try_default()
        .await
        .expect("Should be able to create kube client");
    let k8s_provider = K8sSecretProvider::new(client, "default");
    let raw_secret: EncryptionParameters = k8s_provider.secret_data("my-secret").await?;
    println!("Encryption Parameters from k8s: {:?}", raw_secret);

    Ok(())
}
