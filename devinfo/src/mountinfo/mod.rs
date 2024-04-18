use crate::{
    mountinfo::error::{MountInfoError, Result},
    partition::PartitionID,
};
use io_utils::consistent_read;
use std::{
    ffi::OsString,
    fmt::{self, Display, Formatter},
    fs::File,
    io::{self, BufRead, BufReader, Error, ErrorKind},
    os::unix::prelude::OsStringExt,
    path::{Path, PathBuf},
    str::FromStr,
    sync::OnceLock,
};

/// Errors for MountInfo affairs.
pub mod error;
/// Contains tools to interact with files, etc.
mod io_utils;

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct MountInfo {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub fstype: String,
    pub options: Vec<String>,
}

impl Display for MountInfo {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        write!(
            fmt,
            "{} {} {} {}",
            self.source.display(),
            self.dest.display(),
            self.fstype,
            if self.options.is_empty() {
                "defaults".into()
            } else {
                self.options.join(",")
            },
        )
    }
}

impl FromStr for MountInfo {
    type Err = io::Error;

    fn from_str(line: &str) -> Result<Self, Self::Err> {
        let mut parts = line.split_whitespace();

        fn map_err(why: &'static str) -> io::Error {
            Error::new(ErrorKind::InvalidData, why)
        }

        let source = parts.next().ok_or_else(|| map_err("missing source"))?;
        let dest = parts.next().ok_or_else(|| map_err("missing dest"))?;
        let fstype = parts.next().ok_or_else(|| map_err("missing type"))?;
        let options = parts.next().ok_or_else(|| map_err("missing options"))?;

        let _dump = parts.next().map_or(Ok(0), |value| {
            value
                .parse::<i32>()
                .map_err(|_| map_err("dump value is not a number"))
        })?;

        let _pass = parts.next().map_or(Ok(0), |value| {
            value
                .parse::<i32>()
                .map_err(|_| map_err("pass value is not a number"))
        })?;

        let path = Self::parse_value(source)?;
        let path = path
            .to_str()
            .ok_or_else(|| map_err("non-utf8 paths are unsupported"))?;

        let source = if path.starts_with("/dev/disk/by-") {
            Self::fetch_from_disk_by_path(path)?
        } else {
            PathBuf::from(path)
        };

        let path = Self::parse_value(dest)?;
        let path = path
            .to_str()
            .ok_or_else(|| map_err("non-utf8 paths are unsupported"))?;

        let dest = PathBuf::from(path);

        Ok(MountInfo {
            source,
            dest,
            fstype: fstype.to_owned(),
            options: options.split(',').map(String::from).collect(),
        })
    }
}

impl MountInfo {
    /// Attempt to parse a `/proc/mounts`-like line.

    fn fetch_from_disk_by_path(path: &str) -> io::Result<PathBuf> {
        PartitionID::from_disk_by_path(path)
            .map_err(|why| Error::new(ErrorKind::InvalidData, format!("{path}: {why}")))?
            .get_device_path()
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("device path for {path} was not found"),
                )
            })
    }

    fn parse_value(value: &str) -> io::Result<OsString> {
        let mut ret = Vec::new();

        let mut bytes = value.bytes();
        while let Some(b) = bytes.next() {
            match b {
                b'\\' => {
                    let mut code = 0;
                    for _i in 0 .. 3 {
                        if let Some(b) = bytes.next() {
                            code *= 8;
                            code += u32::from_str_radix(&(b as char).to_string(), 8)
                                .map_err(|err| Error::new(ErrorKind::Other, err))?;
                        } else {
                            return Err(Error::new(ErrorKind::Other, "truncated octal code"));
                        }
                    }
                    ret.push(code as u8);
                }
                _ => {
                    ret.push(b);
                }
            }
        }

        Ok(OsString::from_vec(ret))
    }
}

/// Iteratively parse the `/proc/mounts` file.
pub struct MountIter<R> {
    file: R,
    buffer: String,
}

impl<R: io::Read> MountIter<BufReader<R>> {
    /// Read mounts from any file/buffer, or anything which implements
    /// std::io::Read or event Box<dyn std::io::Read>.
    pub fn new_from_readable(readable: R) -> Self {
        Self::new_from_reader(BufReader::new(readable))
    }
}

impl MountIter<BufReader<File>> {
    pub fn new() -> io::Result<Self> {
        Self::new_from_file("/proc/mounts")
    }

    /// Read mounts from any mount-tab-like file.
    pub fn new_from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Ok(Self::new_from_readable(File::open(path)?))
    }
}

impl<R: BufRead> MountIter<R> {
    /// Read mounts from any in-memory buffer.
    pub fn new_from_reader(readable: R) -> Self {
        Self {
            file: readable,
            buffer: String::with_capacity(512),
        }
    }

    /// Iterator-based variant of `source_mounted_at`.
    ///
    /// Returns true if the `source` is mounted at the given `dest`.
    ///
    /// Due to iterative parsing of the mount file, an error may be returned.
    pub fn source_mounted_at<D: AsRef<Path>, P: AsRef<Path>>(
        source: D,
        path: P,
    ) -> io::Result<bool> {
        let source = source.as_ref();
        let path = path.as_ref();

        let mut is_found = false;

        let mounts = MountIter::new()?;
        for mount in mounts {
            let mount = mount?;
            if mount.source == source {
                is_found = mount.dest == path;
                break;
            }
        }

        Ok(is_found)
    }
}

impl<R: BufRead> Iterator for MountIter<R> {
    type Item = io::Result<MountInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            self.buffer.clear();
            match self.file.read_line(&mut self.buffer) {
                Ok(0) => return None,
                Ok(_) => {
                    let line = self.buffer.trim_start();
                    if !(line.starts_with('#') || line.is_empty()) {
                        return Some(MountInfo::from_str(line));
                    }
                }
                Err(why) => return Some(Err(why)),
            }
        }
    }
}

static SAFE_MOUNT_ITER: OnceLock<SafeMountIter> = OnceLock::new();

/// This returns a Result<Iterator> with reads /proc/mounts consistently.
pub struct SafeMountIter {
    /// This is the flag for the /proc/mounts linux bug. The bug has been fixed in
    /// commit 9f6c61f96f2d97 (v5.8+). This borrows the consistent read solution from
    /// k8s.io/mount-utils. Assumes bug exists by default, if version check fails.
    #[allow(dead_code)]
    kernel_has_mount_info_bug: bool,
    use_safe_mount: bool,
    mounts_filepath: PathBuf,
}

#[test]
fn test_safe_mount() {
    std::env::set_var("USE_SAFE_MOUNT", "always");
    for mount in SafeMountIter::get().unwrap().flatten() {
        println!("mount: {mount}");
    }
}

impl SafeMountIter {
    /// Initialize (if not done already) and get a Result<MountIter>. Retry default no. of times.
    pub fn get() -> Result<MountIter<BufReader<Box<dyn io::Read>>>> {
        Self::get_with_retries(None)
    }

    /// Initialize (if not done already) and get a Result<MountIter>.
    /// More retries make sense if the mount file is likely to see a lot of unmounts often.
    pub fn get_with_retries(
        retries: Option<u32>,
    ) -> Result<MountIter<BufReader<Box<dyn io::Read>>>> {
        // Init.
        let safe_mount_iter = SAFE_MOUNT_ITER.get_or_init(|| {
            use nix::sys::utsname::uname;
            use semver::Version;

            // Bug was fixed in v5.8 with the commit 9f6c61f96f2d97.
            const FIXED_VERSION: Version = Version::new(5, 8, 0);

            let check_uname = || -> Result<bool> {
                let uname = uname()?;

                let release =
                    uname
                        .release()
                        .to_str()
                        .ok_or(MountInfoError::ConvertOsStrToStr {
                            source: uname.release().to_os_string(),
                        })?;
                let version = Version::parse(release)?;

                Ok(version.lt(&FIXED_VERSION))
            };

            // Assume bug exists by default.
            let kernel_has_mount_info_bug = check_uname().unwrap_or(true);
            let use_safe_mount = match std::env::var("USE_SAFE_MOUNT").unwrap_or_default().as_str()
            {
                "always" => true,
                "n" | "no" => false,
                "y" | "yes" => kernel_has_mount_info_bug,
                _ => kernel_has_mount_info_bug,
            };
            Self {
                kernel_has_mount_info_bug,
                use_safe_mount,
                mounts_filepath: PathBuf::from("/proc/mounts"),
            }
        });

        // Decide if consistent read is required.
        if safe_mount_iter.use_safe_mount {
            let buf = consistent_read(safe_mount_iter.mounts_filepath.as_path(), retries)?;
            let buf: Box<dyn io::Read> = Box::new(std::io::Cursor::new(buf));
            return Ok(MountIter::new_from_readable(buf));
        }

        let file = File::open(safe_mount_iter.mounts_filepath.as_path())?;
        let file: Box<dyn io::Read> = Box::new(file);
        Ok(MountIter::new_from_readable(file))
    }
}
