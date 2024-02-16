use crate::mountinfo::error::{MountInfoError, Result};
use std::{fs::read, path::Path};

const DEFAULT_RETRY_COUNT: u32 = 2;

/// Issue std::fs::read on a file twice and compare the two byte sequences
/// to see if the reads were consistent. Retry if the reads resulted in different byte
/// sequences.
pub(crate) fn consistent_read<P: AsRef<Path>>(
    path: P,
    retry_count: Option<u32>,
) -> Result<Vec<u8>> {
    let mut current_content = read(path.as_ref())?;

    let retries = retry_count.unwrap_or(DEFAULT_RETRY_COUNT);
    for _ in 0 ..= retries {
        let new_content = read(path.as_ref())?;

        if new_content.eq(&current_content) {
            return Ok(new_content);
        }

        current_content = new_content;
    }

    Err(MountInfoError::InconsistentRead {
        filepath: path.as_ref().to_path_buf(),
        retries,
    })
}
