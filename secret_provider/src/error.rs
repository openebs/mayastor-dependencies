#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub), context(suffix(false)))]
pub enum Error {
    #[snafu(display("Failed to read file '{file}': {source}"))]
    ReadFile {
        source: std::io::Error,
        file: String,
    },

    #[snafu(display("Failed to parse JSON: {source}"))]
    ParseJson { source: serde_json::Error },

    #[snafu(display("Kubernetes API error: {source}"))]
    KubeApi { source: kube::Error },

    #[snafu(display("Kubernetes secret is missing data"))]
    MissingSecretData {},

    #[snafu(display("Key '{key}' not found in secret data"))]
    MissingSecretKey { key: String },

    #[snafu(display("Failed to convert secret data to UTF-8: {source}"))]
    Utf8Conversion { source: std::str::Utf8Error },
}
