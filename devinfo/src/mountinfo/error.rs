use std::{
    error::Error,
    ffi::OsString,
    fmt::{Debug, Display, Formatter},
    path::PathBuf,
};

pub type Result<T, E = MountInfoError> = std::result::Result<T, E>;

/// Error related to MountInfo, MountIter, SafeMountIter, etc.
#[derive(Debug)]
pub enum MountInfoError {
    Io(std::io::Error),
    InconsistentRead { filepath: PathBuf, retries: u32 },
    Nix(nix::Error),
    ConvertOsStrToStr { source: OsString },
    Semver(semver::Error),
}

impl Display for MountInfoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(ref err) => write!(f, "IO error: {err}"),
            Self::InconsistentRead {
                filepath: path,
                retries,
            } => write!(
                f,
                "failed to get a consistent read output from file {} after {retries} retries",
                path.display()
            ),
            Self::Nix(ref err) => write!(f, "{err}"),
            Self::ConvertOsStrToStr { source: src } => {
                write!(f, "failed to convert {:?} to &str", src)
            }
            Self::Semver(ref err) => write!(f, "semver error: {err}"),
        }
    }
}
impl Error for MountInfoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match *self {
            Self::Io(ref err) => err.source(),
            Self::Nix(ref err) => err.source(),
            Self::Semver(ref err) => err.source(),
            _ => None,
        }
    }
}

impl From<std::io::Error> for MountInfoError {
    fn from(io_err: std::io::Error) -> Self {
        Self::Io(io_err)
    }
}

impl From<nix::Error> for MountInfoError {
    fn from(nix_err: nix::Error) -> Self {
        Self::Nix(nix_err)
    }
}

impl From<semver::Error> for MountInfoError {
    fn from(semver_err: semver::Error) -> Self {
        Self::Semver(semver_err)
    }
}
