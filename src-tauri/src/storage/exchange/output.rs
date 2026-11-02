use crate::error::AppError;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fingerprint {
    size: u64,
    modified: String,
    hash: String,
}

pub(crate) fn fingerprint(path: &Path) -> Result<Option<Fingerprint>, AppError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(AppError::validation("输出目标必须为普通文件"));
    }
    let modified = metadata
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| AppError::validation("文件修改时间无效"))?
        .as_nanos()
        .to_string();
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    Ok(Some(Fingerprint {
        size: metadata.len(),
        modified,
        hash: digest
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    }))
}

pub(super) fn publish(
    temporary: &Path,
    target: &Path,
    expected: Option<&Fingerprint>,
) -> Result<(), AppError> {
    if fingerprint(target)?.as_ref() != expected {
        return Err(AppError::new(
            "OUTPUT_CHANGED",
            "输出目标已改变，请重新选择并确认",
        ));
    }
    if expected.is_none() {
        std::fs::hard_link(temporary, target)?;
        return Ok(());
    }
    replace(temporary, target)
}

#[cfg(windows)]
fn replace(temporary: &Path, target: &Path) -> Result<(), AppError> {
    use std::os::windows::ffi::OsStrExt;
    let source: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe {
        windows_sys::Win32::Storage::FileSystem::ReplaceFileW(
            destination.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if result == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace(temporary: &Path, target: &Path) -> Result<(), AppError> {
    std::fs::rename(temporary, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_checks_original_and_publishes_complete_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("target.json");
        let temporary = directory.path().join("output.tmp");
        std::fs::write(&target, b"old").unwrap();
        std::fs::write(&temporary, b"new-complete").unwrap();
        let expected = fingerprint(&target).unwrap().unwrap();
        let json = serde_json::to_string(&expected).unwrap();
        assert_eq!(
            serde_json::from_str::<Fingerprint>(&json).unwrap(),
            expected
        );
        std::fs::write(&target, b"changed").unwrap();
        assert_eq!(
            publish(&temporary, &target, Some(&expected))
                .unwrap_err()
                .code,
            "OUTPUT_CHANGED"
        );
        assert_eq!(std::fs::read(&target).unwrap(), b"changed");
        let expected = fingerprint(&target).unwrap().unwrap();
        publish(&temporary, &target, Some(&expected)).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new-complete");
    }
}
