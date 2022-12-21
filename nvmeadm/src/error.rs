use snafu::Snafu;

#[derive(Debug, Snafu)]
#[allow(missing_docs)]
#[snafu(visibility(pub(crate)), context(suffix(false)), module(nvme_error))]
pub enum NvmeError {
    #[snafu(display("IO error:"))]
    IoFailed { source: std::io::Error },
    #[snafu(display("Failed to parse {}: {}, {}", path, contents, error))]
    ValueParseFailed {
        path: String,
        contents: String,
        error: String,
    },
    #[snafu(display("Failed to parse value"))]
    ParseFailed {},
    #[snafu(display("File IO error: {}, {}", filename, source))]
    FileIoFailed {
        filename: String,
        source: std::io::Error,
    },
    #[snafu(display("nqn: {} not found", text))]
    NqnNotFound { text: String },
    #[snafu(display("No nvmf subsystems found"))]
    NoSubsystems,
    #[snafu(display(
        "Nvmf subsystem with nqn: {}, host: {}, port: {} not found",
        nqn,
        host,
        port
    ))]
    SubsystemNotFound {
        nqn: String,
        host: String,
        port: u16,
    },
    #[snafu(display("No Nvmf Subsystem found for nqn :{}", nqn))]
    NoSubsytemFound { nqn: String },
    #[snafu(display("Connect in progress"))]
    ConnectInProgress,
    #[snafu(display("NVMe connect failed: {}, {}", filename, source))]
    ConnectFailed {
        source: std::io::Error,
        filename: String,
    },
    #[snafu(display("IO error during NVMe discovery"))]
    NvmeDiscoveryFailed { source: nix::Error },
    #[snafu(display("Controller with nqn: {} not found", text))]
    CtlNotFound { text: String },
    #[snafu(display("Invalid path {}: {}", path, source))]
    InvalidPath {
        source: std::path::StripPrefixError,
        path: String,
    },
    #[snafu(display("NVMe subsystems error: {}, {}", path_prefix, source))]
    SubsystemFailure {
        source: glob::PatternError,
        path_prefix: String,
    },
    #[snafu(display("NVMe URI invalid: {}", source))]
    InvalidUri { source: url::ParseError },
    #[snafu(display("Transport type {} not supported", trtype))]
    TransportNotSupported { trtype: String },
    #[snafu(display("Invalid parameter: {}", text))]
    InvalidParam { text: String },
}

impl From<std::io::Error> for NvmeError {
    fn from(source: std::io::Error) -> NvmeError {
        NvmeError::IoFailed { source }
    }
}
