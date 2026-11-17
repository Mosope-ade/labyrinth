use crate::{Config, Error};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    path::Path,
};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Saved {
    pub version: u8,
    pub refresh_token: String,
    pub subject: Uuid,
}
pub trait Vault {
    fn load(&self) -> Result<Option<String>, Error>;
    fn save(&self, value: &str) -> Result<(), Error>;
    fn clear(&self) -> Result<(), Error>;
}
pub struct Keychain {
    entry: keyring::Entry,
    _lock: File,
}
impl Keychain {
    pub fn open(config: &Config) -> Result<Self, Error> {
        let base = directories::ProjectDirs::from("org", "Labyrinth", "laby")
            .ok_or(Error::CredentialStore)?;
        let dir = base.data_local_dir();
        std::fs::create_dir_all(dir).map_err(|_| Error::CredentialStore)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = std::fs::symlink_metadata(dir).map_err(|_| Error::CredentialStore)?;
            if !meta.is_dir() {
                return Err(Error::CredentialStore);
            }
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|_| Error::CredentialStore)?;
        }
        let lock = lock(&dir.join(format!("{}.lock", config.profile_id())))?;
        let entry = keyring::Entry::new("org.labyrinth.laby.v1", &config.profile_id())
            .map_err(|_| Error::CredentialStore)?;
        Ok(Self { entry, _lock: lock })
    }
}
pub(crate) fn lock(path: &Path) -> Result<File, Error> {
    // Contains no secrets. Reject symlinks; containing directory is user-private.
    if std::fs::symlink_metadata(path).is_ok_and(|m| !m.is_file()) {
        return Err(Error::CredentialStore);
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path).map_err(|_| Error::CredentialStore)?;
    file.try_lock().map_err(|_| Error::Busy)?;
    Ok(file)
}
impl Vault for Keychain {
    fn load(&self) -> Result<Option<String>, Error> {
        match self.entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err(Error::CredentialStore),
        }
    }
    fn save(&self, value: &str) -> Result<(), Error> {
        self.entry
            .set_password(value)
            .map_err(|_| Error::CredentialStore)
    }
    fn clear(&self) -> Result<(), Error> {
        match self.entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(Error::CredentialStore),
        }
    }
}
