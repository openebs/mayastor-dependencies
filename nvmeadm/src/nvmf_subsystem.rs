use crate::{error, nvmf_discovery::TrType, parse_value};
use error::{
    nvme_error::{FileIoFailed, InvalidPath, SubsystemFailure},
    NvmeError,
};
use glob::glob;
use snafu::ResultExt;
use std::{collections::HashMap, fs::OpenOptions, io::Write, path::Path, str::FromStr};

pub const SYSFS_NVME_CTRLR_PREFIX: &str = "/sys/devices/virtual/nvme-fabrics/ctl";

/// Subsystem struct shows us all the connect fabrics. This does not include
/// NVMe devices that are connected by trtype=PCIe.
#[derive(Clone, Debug)]
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
    /// Address contains a comma-separated list of `SubsystemAddrToken`.
    /// Example: traddr=X,trsvcid=Y.
    pub address: SubsystemAddr,
    /// Serial number.
    pub serial: String,
    /// Model number.
    pub model: String,
}

const TR_ADDR: &str = "traddr";
const TR_SVC_ID: &str = "trsvcid";
const SRC_ADDR: &str = "src_addr";

#[derive(Debug, Clone)]
#[allow(unused)]
enum SubsystemAddrToken {
    TrAddr { traddr: String },
    TrSvcId { trsvcid: String },
    SrcAddr { src_addr: String },
    Unknown { key: String, value: String },
}
impl From<(&str, &str)> for SubsystemAddrToken {
    fn from((key, value): (&str, &str)) -> Self {
        let value = value.to_string();
        match key {
            TR_ADDR => Self::TrAddr { traddr: value },
            TR_SVC_ID => Self::TrSvcId { trsvcid: value },
            SRC_ADDR => Self::SrcAddr { src_addr: value },
            unknown => Self::Unknown {
                key: unknown.to_string(),
                value,
            },
        }
    }
}

/// A slightly parsed version of `SubsystemAddr` containing the address tokens as variables
/// making it easier to retrieve them.
#[derive(Debug, Default, Clone)]
pub struct SubsystemAddrExt {
    tr_addr: Option<String>,
    tr_svc_id: Option<String>,
    src_addr: Option<String>,
    unknowns: Vec<SubsystemAddrToken>,
}

impl SubsystemAddrExt {
    /// Check if the subsystem address contains target port.
    pub fn match_host_port(&self, host: &str, port: &str) -> bool {
        self.tr_addr.as_deref() == Some(host) && self.tr_svc_id.as_deref() == Some(port)
    }
}

impl From<SubsystemAddr> for SubsystemAddrExt {
    fn from(value: SubsystemAddr) -> Self {
        Self::from(value.0.as_str())
    }
}
// This would be nice to do with serde, but for now let's not include serde in this crate
impl From<&str> for SubsystemAddrExt {
    fn from(value: &str) -> Self {
        let raw_addr = value.split(',');
        raw_addr.fold(SubsystemAddrExt::default(), |mut acc, addr| {
            if let Some(pair) = addr.split_once('=') {
                match SubsystemAddrToken::from(pair) {
                    SubsystemAddrToken::TrAddr { traddr } => {
                        acc.tr_addr = Some(traddr);
                    }
                    SubsystemAddrToken::TrSvcId { trsvcid } => {
                        acc.tr_svc_id = Some(trsvcid);
                    }
                    SubsystemAddrToken::SrcAddr { src_addr } => {
                        acc.src_addr = Some(src_addr);
                    }
                    addr @ SubsystemAddrToken::Unknown { .. } => acc.unknowns.push(addr),
                }
            }
            acc
        })
    }
}

/// Wrapper over a raw subsystem address string.
/// The address contains a comma-separated list of `SubsystemAddrToken`, example:
/// traddr=10.1.0.2,trsvcid=8420.
#[derive(Clone, Debug)]
pub struct SubsystemAddr(String);

impl SubsystemAddr {
    /// Check if the subsystem address contains target port.
    pub fn match_host_port(&self, host: &str, port: &str) -> bool {
        SubsystemAddrExt::from(self.as_str()).match_host_port(host, port)
    }
    /// Collect all address variants into a key-val map.
    pub fn collect(&self) -> HashMap<&str, &str> {
        self.0
            .split(',')
            .filter_map(|s| s.split_once('='))
            .collect::<HashMap<&str, &str>>()
    }
    /// SubsystemAddr content as slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// SubsystemAddr content as raw String.
    pub fn to_raw(self) -> String {
        self.0
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
                path: format!("{source:?}"),
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
            address: SubsystemAddr(address),
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
            .open(path)
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
            .open(path)
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
            .open(path)
            .context(FileIoFailed {
                filename: &filename,
            })?;
        file.write_all(b"1").context(FileIoFailed { filename })?;
        Ok(())
    }

    /// Returns the particular subsystem based on the nqn and address.
    // TODO: Optimize this code.
    pub fn get(
        host: &str,
        port: &u16,
        transport: TrType,
        nqn: &str,
    ) -> Result<Subsystem, NvmeError> {
        let nvme_subsystems = NvmeSubsystems::new()?;

        let host = host.to_string();
        let sport = port.to_string();
        let trstring = transport.to_string();
        match nvme_subsystems.flatten().find(|subsys| {
            subsys.nqn == *nqn
                && subsys.address.match_host_port(&host, &sport)
                && subsys.transport.eq_ignore_ascii_case(trstring.as_str())
        }) {
            None => Err(NvmeError::SubsystemNotFound {
                nqn: nqn.to_string(),
                host: host.to_string(),
                port: *port,
            }),
            Some(subsys) => Ok(subsys),
        }
    }

    /// Gets all Nvme subsystem registered for a given nqn.
    pub fn try_from_nqn(nqn: &str) -> Result<Vec<Subsystem>, NvmeError> {
        let nvme_subsystems = NvmeSubsystems::new()?;
        let mut nvme_paths = vec![];
        for path in nvme_subsystems.flatten() {
            if path.nqn == nqn {
                nvme_paths.push(path);
            }
        }
        if nvme_paths.is_empty() {
            Err(NvmeError::NqnNotFound {
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
            .map(|p| p.display().to_string())
            .collect();
        Ok(NvmeSubsystems { entries })
    }
}
