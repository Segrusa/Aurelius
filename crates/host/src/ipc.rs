//! User-only local IPC: named pipes on Windows, Unix domain sockets on macOS.
//! No network listener.

use crate::frame::{self, FrameError};
use serde::{Serialize, de::DeserializeOwned};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint(pub String);

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type BoxStream = Box<dyn Stream>;

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("connection error: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// Read one frame; `Ok(None)` on a clean end of stream.
pub async fn read_frame<T: DeserializeOwned, R: AsyncRead + Unpin + ?Sized>(reader: &mut R) -> Result<Option<T>, IpcError> {
    let mut header = [0u8; 4];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(err) => return Err(err.into()),
    }
    // Validate size before allocating (oversized frames are refused).
    let len = frame::check_len(header)?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(Some(frame::decode(&body)?))
}

pub async fn write_frame<T: Serialize, W: AsyncWrite + Unpin + ?Sized>(writer: &mut W, message: &T) -> Result<(), IpcError> {
    let bytes = frame::encode(message)?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};

    pub struct Listener {
        name: String,
        next: NamedPipeServer,
        security: crate::windows_security::UserOnlySecurity,
    }

    fn create(name: &str, first: bool, security: &crate::windows_security::UserOnlySecurity) -> io::Result<NamedPipeServer> {
        let mut options = ServerOptions::new();
        options.first_pipe_instance(first).reject_remote_clients(true);
        // SAFETY: the security attributes live in `security`, which outlives this call.
        unsafe { options.create_with_security_attributes_raw(name, security.as_ptr()) }
    }

    impl Listener {
        pub fn bind(endpoint: &Endpoint) -> io::Result<Self> {
            let security = crate::windows_security::UserOnlySecurity::new()?;
            // first_pipe_instance(true) fails if another process already owns the name.
            let next = create(&endpoint.0, true, &security)?;
            Ok(Self { name: endpoint.0.clone(), next, security })
        }

        pub async fn accept(&mut self) -> io::Result<BoxStream> {
            self.next.connect().await?;
            let fresh = create(&self.name, false, &self.security)?;
            let connected = std::mem::replace(&mut self.next, fresh);
            Ok(Box::new(connected))
        }
    }

    pub async fn connect(endpoint: &Endpoint) -> io::Result<BoxStream> {
        const ERROR_PIPE_BUSY: i32 = 231;
        for _ in 0..50 {
            match ClientOptions::new().open(&endpoint.0) {
                Ok(client) => return Ok(Box::new(client)),
                Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(err) => return Err(err),
            }
        }
        Err(io::Error::new(io::ErrorKind::TimedOut, "daemon pipe stayed busy"))
    }
}

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use tokio::net::{UnixListener, UnixStream};

    pub struct Listener {
        inner: UnixListener,
    }

    impl Listener {
        /// Caller must hold the instance lock, so removing a stale socket is safe.
        pub fn bind(endpoint: &Endpoint) -> io::Result<Self> {
            let path = PathBuf::from(&endpoint.0);
            let dir = path.parent().expect("socket has a directory");
            crate::ensure_private_dir(dir)?;
            let _ = std::fs::remove_file(&path);
            let inner = UnixListener::bind(&path)?;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            Ok(Self { inner })
        }

        pub async fn accept(&mut self) -> io::Result<BoxStream> {
            let (stream, _) = self.inner.accept().await?;
            // Defense in depth: only the same OS user may talk to the daemon.
            let peer = stream.peer_cred()?;
            if peer.uid() != current_uid() {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "peer is a different user"));
            }
            Ok(Box::new(stream))
        }
    }

    fn current_uid() -> u32 {
        unsafe extern "C" {
            fn getuid() -> u32;
        }
        // SAFETY: getuid has no preconditions and cannot fail.
        unsafe { getuid() }
    }

    pub async fn connect(endpoint: &Endpoint) -> io::Result<BoxStream> {
        Ok(Box::new(UnixStream::connect(&endpoint.0).await?))
    }
}

pub use imp::{Listener, connect};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn oversized_frame_is_refused_before_allocation() {
        let mut bytes = ((frame::MAX_FRAME_BYTES + 1) as u32).to_be_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        let mut reader = bytes.as_slice();
        let err = read_frame::<serde_json::Value, _>(&mut reader).await.unwrap_err();
        assert!(matches!(err, IpcError::Frame(FrameError::TooLarge(_))));
    }

    #[tokio::test]
    async fn malformed_frame_is_an_error_and_eof_is_clean() {
        let mut bytes = 3u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"{x}");
        let mut reader = bytes.as_slice();
        assert!(matches!(read_frame::<serde_json::Value, _>(&mut reader).await, Err(IpcError::Frame(_))));
        let mut empty: &[u8] = &[];
        assert!(read_frame::<serde_json::Value, _>(&mut empty).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn listener_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let endpoint = crate::endpoint(dir.path());
        let mut listener = Listener::bind(&endpoint).unwrap();
        let server = tokio::spawn(async move {
            let mut stream = listener.accept().await.unwrap();
            let value: serde_json::Value = read_frame(&mut stream).await.unwrap().unwrap();
            write_frame(&mut stream, &value).await.unwrap();
        });
        let mut client = connect(&endpoint).await.unwrap();
        write_frame(&mut client, &serde_json::json!({"ping": 1})).await.unwrap();
        let echo: serde_json::Value = read_frame(&mut client).await.unwrap().unwrap();
        assert_eq!(echo["ping"], 1);
        server.await.unwrap();
    }
}
