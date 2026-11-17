use crate::{Client, Error, Vault};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::Path,
};
use uuid::Uuid;
const FILES: [&str; 13] = [
    "bom.json",
    "catalog.json",
    "decisions.json",
    "firewall.json",
    "ipam.json",
    "manifest.json",
    "options.json",
    "provenance.json",
    "requirements.json",
    "routing.json",
    "selection.json",
    "topology.json",
    "verification.json",
];
#[derive(Deserialize)]
struct Artifact {
    name: String,
    sha256: String,
    size_bytes: usize,
}
#[derive(Deserialize)]
struct Revision {
    revision_id: Uuid,
    manifest_sha256: String,
    artifacts: Vec<Artifact>,
}
#[derive(Deserialize)]
struct DigestEntry {
    sha256: String,
    size_bytes: usize,
}
#[derive(Deserialize)]
struct Manifest {
    schema_version: String,
    files: BTreeMap<String, DigestEntry>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Client {
    pub async fn download(
        &self,
        vault: &dyn Vault,
        tenant: Uuid,
        project: Uuid,
        revision: Uuid,
        out: &Path,
    ) -> Result<(), Error> {
        if out.exists() || out.symlink_metadata().is_ok() {
            return Err(Error::Output);
        }
        let base = format!("v1/tenants/{tenant}/projects/{project}/revisions/{revision}");
        let token = self.access(vault).await?;
        let metadata = self.bytes(&token, &base, 65536).await?;
        let metadata: Revision = serde_json::from_slice(&metadata).map_err(|_| Error::Integrity)?;
        if metadata.revision_id != revision
            || !valid_hash(&metadata.manifest_sha256)
            || metadata.artifacts.len() != FILES.len()
        {
            return Err(Error::Integrity);
        }
        let mut total = 0usize;
        let mut seen = BTreeSet::new();
        for artifact in &metadata.artifacts {
            total = total
                .checked_add(artifact.size_bytes)
                .ok_or(Error::Integrity)?;
            if !FILES.contains(&artifact.name.as_str())
                || !seen.insert(artifact.name.clone())
                || !valid_hash(&artifact.sha256)
                || artifact.size_bytes == 0
                || total > 32 * 1024 * 1024
            {
                return Err(Error::Integrity);
            }
        }
        let mut files = BTreeMap::new();
        for artifact in &metadata.artifacts {
            let bytes = self
                .bytes(
                    &token,
                    &format!("{base}/artifacts/{}", artifact.name),
                    artifact.size_bytes,
                )
                .await?;
            if bytes.len() != artifact.size_bytes || digest(&bytes) != artifact.sha256 {
                return Err(Error::Integrity);
            }
            if serde_json::from_slice::<serde_json::Value>(&bytes).is_err() {
                return Err(Error::Integrity);
            }
            files.insert(artifact.name.clone(), bytes);
        }
        let manifest = files.get("manifest.json").ok_or(Error::Integrity)?;
        if digest(manifest) != metadata.manifest_sha256 {
            return Err(Error::Integrity);
        }
        let manifest: Manifest = serde_json::from_slice(manifest).map_err(|_| Error::Integrity)?;
        if manifest.schema_version != "engineering_package_v1"
            || manifest.files.len() != FILES.len() - 1
        {
            return Err(Error::Integrity);
        }
        for (name, entry) in &manifest.files {
            if name == "manifest.json" {
                return Err(Error::Integrity);
            }
            let bytes = files.get(name).ok_or(Error::Integrity)?;
            if bytes.len() != entry.size_bytes || digest(bytes) != entry.sha256 {
                return Err(Error::Integrity);
            }
        }
        save(out, &files)
    }
}
fn save(out: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), Error> {
    let dir = std::fs::DirBuilder::new();
    #[cfg(unix)]
    let dir = {
        use std::os::unix::fs::DirBuilderExt;
        let mut dir = dir;
        dir.mode(0o700);
        dir
    };
    dir.create(out).map_err(|_| Error::Output)?;
    let result = (|| {
        for (name, bytes) in files
            .iter()
            .filter(|(name, _)| name.as_str() != "manifest.json")
            .chain(
                files
                    .iter()
                    .filter(|(name, _)| name.as_str() == "manifest.json"),
            )
        {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(out.join(name)).map_err(|_| Error::Output)?;
            file.write_all(bytes).map_err(|_| Error::Output)?;
            file.sync_all().map_err(|_| Error::Output)?;
        }
        #[cfg(unix)]
        std::fs::File::open(out)
            .and_then(|dir| dir.sync_all())
            .map_err(|_| Error::Output)?;
        Ok(())
    })();
    if result.is_err() {
        for name in files.keys() {
            let _ = std::fs::remove_file(out.join(name));
        }
        let _ = std::fs::remove_dir(out);
    }
    result
}
