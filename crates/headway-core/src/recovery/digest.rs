use crate::error::{Error, Result};
use sha2::{Digest as _, Sha256};
use std::fs;
use std::path::Path;

pub(crate) fn digest(path: &Path) -> Result<String> {
    let mut hash = Sha256::new();
    hash_path(path, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

fn hash_path(path: &Path, hash: &mut Sha256) -> Result<()> {
    use std::io::Read as _;
    use std::os::unix::fs::PermissionsExt as _;
    let meta = fs::symlink_metadata(path).map_err(|e| Error::io_at("read", path, e))?;
    hash.update(meta.permissions().mode().to_le_bytes());
    if meta.is_symlink() {
        hash.update(b"link");
        hash.update(
            fs::read_link(path)
                .map_err(|e| Error::io_at("read link", path, e))?
                .as_os_str()
                .as_encoded_bytes(),
        );
    } else if meta.is_dir() {
        hash.update(b"directory");
        let mut children = fs::read_dir(path)
            .map_err(|e| Error::io_at("read", path, e))?
            .collect::<std::io::Result<Vec<_>>>()
            .map_err(|e| Error::io_at("read", path, e))?;
        children.sort_by_key(std::fs::DirEntry::file_name);
        for child in children {
            hash.update(child.file_name().as_encoded_bytes());
            hash.update([0]);
            hash_path(&child.path(), hash)?;
        }
    } else if meta.is_file() {
        hash.update(b"file");
        hash.update(meta.len().to_le_bytes());
        let mut file = fs::File::open(path).map_err(|e| Error::io_at("read", path, e))?;
        let mut buffer = [0; 8192];
        loop {
            let count = file
                .read(&mut buffer)
                .map_err(|e| Error::io_at("read", path, e))?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
    } else {
        return Err(Error::InvalidInput(
            "unsupported backup payload type".into(),
        ));
    }
    Ok(())
}
