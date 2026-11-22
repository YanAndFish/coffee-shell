//! The server's ed25519 host key: read from disk, or made (mode 0600) on first run.

use std::{
  fs,
  io::{self, Write},
  os::unix::fs::{OpenOptionsExt, PermissionsExt},
  path::Path,
};

use russh::keys::{Algorithm, PrivateKey, ssh_key::LineEnding};

/// Reads the OpenSSH private key at `path`, or makes an ed25519 one there.
pub fn load_or_generate(path: &Path) -> io::Result<(PrivateKey, bool)> {
  if path.exists() {
    let mode = fs::metadata(path)?.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
      tracing::warn!(
        path = %path.display(),
        mode = format!("{mode:o}"),
        "host key is readable by others; chmod 600 it"
      );
    }
    let text = fs::read_to_string(path)?;
    let key = PrivateKey::from_openssh(text.as_bytes())
      .map_err(|error| io::Error::other(format!("{}: {error}", path.display())))?;
    return Ok((key, false));
  }
  let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
    .map_err(|error| io::Error::other(format!("generating the host key: {error}")))?;
  let text = key
    .to_openssh(LineEnding::LF)
    .map_err(|error| io::Error::other(format!("encoding the host key: {error}")))?;
  if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
    fs::create_dir_all(parent)?;
  }
  let mut file = fs::OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(path)?;
  file.write_all(text.as_bytes())?;
  file.sync_all()?;
  Ok((key, true))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_key_is_made_once_with_owner_only_access() {
    let dir = std::env::temp_dir().join(format!("coffee-sshd-hostkey-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("nested/ssh_host_ed25519_key");
    let (made, fresh) = load_or_generate(&path).unwrap();
    assert!(fresh);
    assert_eq!(made.algorithm(), Algorithm::Ed25519);
    assert_eq!(
      fs::metadata(&path).unwrap().permissions().mode() & 0o777,
      0o600
    );
    let (read, fresh) = load_or_generate(&path).unwrap();
    assert!(!fresh);
    assert_eq!(read.public_key(), made.public_key());
    fs::remove_dir_all(&dir).unwrap();
  }
}
