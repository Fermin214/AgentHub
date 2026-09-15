//! Bounded directory operations shared by Skill import, changes, and restoration.
use crate::{native, protection, store::Store};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Read,
    path::{Component, Path},
};

fn plain_metadata(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if metadata.file_attributes() & 0x400 != 0 {
                    bail!("目录经过链接或重解析点：{}", path.display());
                }
            }
            if metadata.file_type().is_symlink() {
                bail!("不操作符号链接：{}", path.display());
            }
            Ok(Some(metadata))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

/// Validate a directory and each ancestor without recursively scanning unrelated data.
/// A missing destination is allowed; tree readers/writers inspect every child separately.
pub fn safe_directory(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        bail!("目录必须是没有上级跳转的完整路径");
    }
    for parent in path.ancestors() {
        if let Some(metadata) = plain_metadata(parent)? {
            if !metadata.is_dir() {
                bail!("路径不是目录：{}", parent.display());
            }
        }
    }
    Ok(())
}

/// Read a small regular file without following linked files or ancestors.
pub(crate) fn read_small_text(path: &Path, limit: u64) -> Result<String> {
    safe_directory(path.parent().context("文件缺少父目录")?)?;
    let metadata = plain_metadata(path)?.context("文件不存在")?;
    if !metadata.is_file() || metadata.len() > limit {
        bail!("文件不是普通文本或超过大小上限");
    }
    let mut text = String::new();
    File::open(path)?
        .take(limit + 1)
        .read_to_string(&mut text)?;
    if text.len() as u64 > limit {
        bail!("文件超过大小上限");
    }
    Ok(text)
}

pub fn digest(path: &Path) -> Result<String> {
    safe_directory(path)?;
    if !path.exists() {
        return Ok("absent".into());
    }
    native::inspect_tree(path)?;
    let mut hash = Sha256::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry?;
        crate::source_control::checkpoint()?;
        let metadata = plain_metadata(entry.path())?.context("摘要读取期间文件消失")?;
        let relative = entry
            .path()
            .strip_prefix(path)?
            .to_string_lossy()
            .replace('\\', "/");
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        hash.update([u8::from(metadata.is_file())]);
        if metadata.is_file() {
            hash.update(metadata.len().to_le_bytes());
            let mut file = File::open(entry.path())?;
            let mut buffer = [0u8; 65536];
            loop {
                crate::source_control::checkpoint()?;
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total = total.checked_add(count as u64).context("Skill 大小溢出")?;
                if total > 512 * 1024 * 1024 {
                    bail!("Skill 超过大小上限");
                }
                hash.update(&buffer[..count]);
            }
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn lock(store: &Store) -> Result<File> {
    safe_directory(store.data_dir())?;
    fs::create_dir_all(store.data_dir())?;
    let path = store.data_dir().join("skill-write.lock");
    if plain_metadata(&path)?.is_some_and(|metadata| !metadata.is_file()) {
        bail!("Skill 操作锁不是普通文件");
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path)?;
    file.try_lock()
        .context("另一个 Skill 操作正在执行，请稍后重试")?;
    Ok(file)
}
/// Short local critical sections wait for each other; network reads never hold
/// this lock. Keep the existing immediate lock for user file mutations.
pub(crate) fn lock_wait(store: &Store) -> Result<File> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match lock(store) {
            Ok(file) => return Ok(file),
            Err(error) => {
                if !error.chain().any(|e| {
                    e.downcast_ref::<std::fs::TryLockError>()
                        .is_some_and(|e| matches!(e, std::fs::TryLockError::WouldBlock))
                }) || std::time::Instant::now() >= deadline
                {
                    return Err(error);
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        }
    }
}

pub fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    safe_directory(source)?;
    safe_directory(destination)?;
    protection::ensure_mutable(destination)?;
    if !source.is_dir() || destination.exists() {
        bail!("复制要求已有来源目录和不存在的目标目录");
    }
    let source_key = protection::path_key(source);
    let destination_key = protection::path_key(destination);
    if destination_key == source_key || destination_key.starts_with(&(source_key + "/")) {
        bail!("不能把 Skill 复制到自身内部");
    }
    let before = digest(source)?;
    native::copy_bounded(source, destination)?;
    if digest(source)? != before || digest(destination)? != before {
        bail!("复制期间 Skill 文件变化，请重新检查");
    }
    Ok(())
}

pub fn remove_tree(path: &Path) -> Result<()> {
    safe_directory(path)?;
    protection::ensure_mutable(path)?;
    if path.file_name().is_none() || path.components().count() < 3 {
        bail!("不能移除文件系统根目录");
    }
    if !path.exists() {
        return Ok(());
    }
    native::inspect_tree(path)?;
    fs::remove_dir_all(path).with_context(|| format!("移除目录 {}", path.display()))
}
