use crate::{error, parse_value};
use error::{
    nvme_error::{FileIoFailed, InvalidPath, SubsystemFailure},
    NvmeError,
};
use glob::glob;
use snafu::ResultExt;
use std::{fs::OpenOptions, io::Write, path::Path, str::FromStr};

pub const SYSFS_NVME_CTRLR_PREFIX: &str = "/sys/devices/virtual/nvme-fabrics/ctl";

/// Subsystem struct shows us all the connect fabrics. This does not include
/// NVMe devices that are connected by trtype=PCIe.
#[derive(Default, Clone, Debug)]
pub struct Subsystem {
    /// Name of the subsystem.
    pub name: String,
    /// Instance number of the subsystem (controller).
    pub instance: u32,
    /// NVme Qualified Name (NQN).
    pub nqn: String,
    /// State of the connection, will contain live if online.
    pub state: String,
    /// The transport type being used (tcp or RDMA).
    pub transport: String,
    /// Address contains traddr=X,trsvcid=Y.
    pub address: String,
    /// Serial number.
    pub serial: String,
    /// Model number.
    pub model: String,
}

/// Wrapper structure that creates subsystem addr string.
pub struct SubsystemAddr(String);

impl SubsystemAddr {
    /// New SubsystemAddr.
    pub fn new(host: String, port: u16) -> SubsystemAddr {
        SubsystemAddr(format!("traddr={},trsvcid={}", host, port))
    }
    /// SubsystemAddr content as slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// For SubsystemAddr == String comparisons
impl PartialEq<String> for SubsystemAddr {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

// For String == SubsystemAddr comparisons
impl PartialEq<SubsystemAddr> for String {
    fn eq(&self, other: &SubsystemAddr) -> bool {
        self == other.as_str()
    }
}

impl Subsystem {
    /// Scans the sysfs directory for attached subsystems skips any transport
    /// that does not contain a value that is being read in the implementation.
    pub fn new(source: &Path) -> Result<Self, NvmeError> {
        let name = source
            .strip_prefix(SYSFS_NVME_CTRLR_PREFIX)
            .context(InvalidPath {
                path: format!("{:?}", source),
            })?
            .display()
            .to_string();
        let instance = u32::from_str(name.trim_start_matches("nvme")).unwrap();
        let nqn = parse_value::<String>(source, "subsysnqn")?;
        let state = parse_value::<String>(source, "state")?;
        let transport = parse_value::<String>(source, "transport")?;
        let address = parse_value::<String>(source, "address")?;
        let serial = parse_value::<String>(source, "serial")?;
        let model = parse_value::<String>(source, "model")?;

        if serial.is_empty() || model.is_empty() {
            return Err(NvmeError::CtlNotFound {
                text: "discovery controller".into(),
            });
        }

        // if it does not have a serial and or model -- its a discovery
        // controller so we skip it

        let model = parse_value::<String>(source, "model")?;
        Ok(Subsystem {
            name,
            instance,
            nqn,
            state,
            transport,
            address,
            serial,
            model,
        })
    }
    /// Synchronize in-memory state of this subsystem with system's state.
    /// The folowing is updated: state
    pub fn sync(&mut self) -> Result<(), NvmeError> {
        let filename = format!("{}/{}", SYSFS_NVME_CTRLR_PREFIX, self.name);
        let path = Path::new(&filename);
        let state = parse_value::<String>(path, "state")?;

        self.state = state;
        Ok(())
    }

    /// Issue a rescan to the controller to find new namespaces.
    pub fn rescan(&self) -> Result<(), NvmeError> {
        let filename = format!("/sys/class/nvme/{}/rescan_controller", self.name);
        let path = Path::new(&filename);

        let mut file = OpenOptions::new()
            .write(true)
            .open(&path)
            .context(FileIoFailed {
                filename: &filename,
            })?;
        file.write_all(b"1").context(FileIoFailed { filename })?;
        Ok(())
    }
    /// Disconnects the transport dropping all namespaces.
    pub fn disconnect(&self) -> Result<(), NvmeError> {
        let filename = format!("/sys/class/nvme/{}/delete_controller", self.name);
        let path = Path::new(&filename);

        let mut file = OpenOptions::new()
            .write(true)
            .open(&path)
            .context(FileIoFailed {
                filename: &filename,
            })?;
        file.write_all(b"1").context(FileIoFailed { filename })?;
        Ok(())
    }
    /// Resets the nvme controller.
    pub fn reset(&self) -> Result<(), NvmeError> {
        let filename = format!("/sys/class/nvme/{}/reset_controller", self.name);
        let path = Path::new(&filename);

        let mut file = OpenOptions::new()
            .write(true)
            .open(&path)
            .context(FileIoFailed {
                filename: &filename,
            })?;
        file.write_all(b"1").context(FileIoFailed { filename })?;
        Ok(())
    }

    /// Returns the particular subsystem based on the nqn and address.
    // TODO: Optimize this code.
    pub fn get(host: &str, port: &u16, nqn: &str) -> Result<Subsystem, NvmeError> {
        let address = SubsystemAddr::new(host.to_string(), *port);

        let nvme_subsystems = NvmeSubsystems::new()?;

        match nvme_subsystems
            .flatten()
            .find(|subsys| subsys.nqn == *nqn && subsys.address == address)
        {
            None => Err(NvmeError::SubsystemNotFound {
                nqn: nqn.to_string(),
                host: host.to_string(),
                port: *port,
            }),
            Some(subsys) => Ok(subsys),
        }
    }

    /// Gets all Nvme subsystems registered for a given volume.
    pub fn get_from_nqn(nqn: &str) -> Result<Vec<Subsystem>, NvmeError> {
        let nvme_subsystems = NvmeSubsystems::new()?;
        let mut nvme_paths = vec![];
        for path in nvme_subsystems.flatten() {
            if path.nqn == nqn && (path.state == "live" || path.state == "connecting") {
                nvme_paths.push(path);
            }
        }
        if !nvme_paths.is_empty() {
            Err(NvmeError::NoSubsytemFound {
                nqn: nqn.to_string(),
            })
        } else {
            Ok(nvme_paths)
        }
    }
}

/// List of subsystems found on the system.
#[derive(Default, Debug)]
pub struct NvmeSubsystems {
    entries: Vec<String>,
}

impl Iterator for NvmeSubsystems {
    type Item = Result<Subsystem, NvmeError>;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(e) = self.entries.pop() {
            return Some(Subsystem::new(Path::new(&e)));
        }
        None
    }
}

impl NvmeSubsystems {
    /// Construct a new list of subsystems.
    pub fn new() -> Result<Self, NvmeError> {
        let path_prefix = "/sys/devices/virtual/nvme-fabrics/ctl/nvme*";
        let path_entries = glob(path_prefix).context(SubsystemFailure { path_prefix })?;
        let entries = path_entries
            .flatten()
            .into_iter()
            .map(|p| p.display().to_string())
            .collect();
        Ok(NvmeSubsystems { entries })
    }
}
