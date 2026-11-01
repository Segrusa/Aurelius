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
    /// The name the service on this computer last gave (feature `server_name`),
    /// shown next to "This computer"; none before the first connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_name: Option<String>,
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

    /// Take the name a server gave (its hello or `server.renamed`) for the saved
    /// server with this host id (SN-02 AC2). True when the list changed.
    pub fn rename(&mut self, host_id: HostId, name: &str) -> bool {
        let name = name.trim();
        match self.servers.iter_mut().find(|s| s.host_id == host_id) {
            Some(saved) if !name.is_empty() && saved.host_name != name => {
                saved.host_name = name.to_owned();
                true
            }
            _ => false,
        }
    }

    /// Take the name the service on this computer gave. True when it changed.
    pub fn rename_local(&mut self, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() || self.local_name.as_deref() == Some(name) {
            return false;
        }
        self.local_name = Some(name.to_owned());
        true
    }

    /// Store the name a server gave in `<data>/client/servers.json`: by host id,
    /// and as the local name when it is the service on this computer. The file
    /// is written only when something changed; answers the list as it is now.
    pub fn remember_name(root: &Path, host_id: HostId, name: &str, local: bool) -> io::Result<Self> {
        let mut servers = Self::load(root);
        let renamed = servers.rename(host_id, name);
        let local = local && servers.rename_local(name);
        if renamed || local {
            servers.save(root)?;
        }
        Ok(servers)
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

    fn saved(host_id: HostId, name: &str) -> SavedServer {
        SavedServer { host_id, host_name: name.into(), url: format!("https://{name}:8443"), client_id: ClientId::new(), environment: "dev".into(), last_used_ms: 1 }
    }

    #[test]
    fn a_new_name_overwrites_the_saved_one_with_the_same_host_id_only() {
        let dir = tempfile::tempdir().unwrap();
        let (office, home) = (HostId::new(), HostId::new());
        let servers = Servers { active: None, servers: vec![saved(office, "desktop-1"), saved(home, "laptop")], local_name: None };
        servers.save(dir.path()).unwrap();

        let now = Servers::remember_name(dir.path(), office, " Office ", false).unwrap();
        assert_eq!(now.find(office).unwrap().host_name, "Office", "trimmed and stored");
        assert_eq!(now.find(home).unwrap().host_name, "laptop", "another host id keeps its name");
        assert_eq!(Servers::load(dir.path()), now, "the file has it, also for a server not used afterwards");
        assert_eq!(now.local_name, None, "a remote server is not this computer");
    }

    #[test]
    fn the_same_or_an_empty_name_does_not_write_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let office = HostId::new();
        Servers { active: Some(office), servers: vec![saved(office, "Office")], local_name: None }.save(dir.path()).unwrap();
        let file = path(dir.path());
        let before = std::fs::metadata(&file).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        Servers::remember_name(dir.path(), office, "Office", false).unwrap();
        Servers::remember_name(dir.path(), office, "  ", false).unwrap();
        Servers::remember_name(dir.path(), HostId::new(), "Other", false).unwrap();
        assert_eq!(std::fs::metadata(&file).unwrap().modified().unwrap(), before);
        assert_eq!(Servers::load(dir.path()).find(office).unwrap().host_name, "Office");
    }

    #[test]
    fn the_local_service_keeps_its_last_name_for_the_this_computer_row() {
        let dir = tempfile::tempdir().unwrap();
        let now = Servers::remember_name(dir.path(), HostId::new(), "Kantoor", true).unwrap();
        assert_eq!(now.local_name.as_deref(), Some("Kantoor"));
        assert_eq!(Servers::load(dir.path()).local_name.as_deref(), Some("Kantoor"));
    }

    #[test]
    fn a_file_without_a_local_name_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("client")).unwrap();
        std::fs::write(path(dir.path()), r#"{"active":null,"servers":[]}"#).unwrap();
        assert_eq!(Servers::load(dir.path()), Servers::default());
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
