//! The servers this computer is paired with, and which one it uses
//! (`<data dir>/client/servers.json`). Device secrets live in the OS
//! credential store (`server/<host_id>`), never in the file.

use crate::remote::RemoteServer;
use aurelius_domain::{ClientId, HostId};
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedServer {
    pub host_id: HostId,
    pub host_name: String,
    pub url: String,
    pub client_id: ClientId,
    pub environment: String,
    #[serde(default)]
    pub last_used_ms: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Servers {
    /// The server to use; `None` = the service on this computer.
    pub active: Option<HostId>,
    pub servers: Vec<SavedServer>,
}

fn path(root: &Path) -> PathBuf {
    root.join("client").join("servers.json")
}

fn secret_ref(host_id: HostId) -> String {
    format!("server/{host_id}")
}

impl Servers {
    pub fn load(root: &Path) -> Self {
        std::fs::read(path(root)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let file = path(root);
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(tmp, file)
    }

    pub fn find(&self, host_id: HostId) -> Option<&SavedServer> {
        self.servers.iter().find(|s| s.host_id == host_id)
    }

    /// Remember a freshly paired server (replacing an earlier pairing with it)
    /// and make it the active one.
    pub fn add(&mut self, server: &RemoteServer) -> io::Result<()> {
        aurelius_host::secrets::set(&secret_ref(server.host_id), &server.secret)?;
        self.servers.retain(|s| s.host_id != server.host_id);
        self.servers.push(SavedServer {
            host_id: server.host_id,
            host_name: server.host_name.clone(),
            url: server.url.clone(),
            client_id: server.client_id,
            environment: server.environment.clone(),
            last_used_ms: now_ms(),
        });
        self.active = Some(server.host_id);
        Ok(())
    }

    /// Forget a server and its secret; back to this computer if it was active.
    pub fn forget(&mut self, host_id: HostId) -> io::Result<()> {
        self.servers.retain(|s| s.host_id != host_id);
        if self.active == Some(host_id) {
            self.active = None;
        }
        aurelius_host::secrets::delete(&secret_ref(host_id))
    }

    pub fn touch(&mut self, host_id: HostId) {
        if let Some(saved) = self.servers.iter_mut().find(|s| s.host_id == host_id) {
            saved.last_used_ms = now_ms();
        }
    }

    /// The saved server with its secret, ready to connect.
    pub fn credentials(&self, host_id: HostId) -> io::Result<RemoteServer> {
        let saved = self.find(host_id).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "this server is not paired"))?;
        let secret = aurelius_host::secrets::get(&secret_ref(host_id))?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "the pairing secret is missing; pair again"))?;
        Ok(RemoteServer {
            url: saved.url.clone(),
            host_id: saved.host_id,
            host_name: saved.host_name.clone(),
            client_id: saved.client_id,
            secret,
            environment: saved.environment.clone(),
        })
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_roundtrips_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let mut servers = Servers::default();
        let host_id = HostId::new();
        servers.servers.push(SavedServer {
            host_id,
            host_name: "my-pc".into(),
            url: "https://my-pc.tail1.ts.net:8443".into(),
            client_id: ClientId::new(),
            environment: "dev".into(),
            last_used_ms: 1,
        });
        servers.active = Some(host_id);
        servers.save(dir.path()).unwrap();
        assert_eq!(Servers::load(dir.path()), servers);
        let text = std::fs::read_to_string(path(dir.path())).unwrap();
        assert!(!text.contains("secret"));
    }

    #[test]
    fn a_missing_or_broken_file_means_this_computer() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Servers::load(dir.path()), Servers::default());
        std::fs::create_dir_all(dir.path().join("client")).unwrap();
        std::fs::write(path(dir.path()), "{not json").unwrap();
        assert_eq!(Servers::load(dir.path()).active, None);
    }
}
