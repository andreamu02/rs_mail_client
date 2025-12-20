use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::net::UnixStream;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    SyncPage { page: u32, page_size: u32 },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    pub message: Option<String>,
}

pub fn socket_path() -> Result<PathBuf> {
    let base = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("no config dir"))?
        .join("rs_mail_client");
    std::fs::create_dir_all(&base)?;
    Ok(base.join("daemon.sock"))
}

#[cfg(unix)]
pub fn send(req: &Request) -> Result<Response> {
    let path = socket_path()?;
    let mut s = UnixStream::connect(path)?;
    let data = serde_json::to_vec(req)?;
    // length-prefix
    s.write_all(&(data.len() as u32).to_be_bytes())?;
    s.write_all(&data)?;
    s.flush()?;

    let mut len_buf = [0u8; 4];
    s.read_exact(&mut len_buf)?;
    let n = u32::from_be_bytes(len_buf) as usize;
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf)?;
    Ok(serde_json::from_slice(&buf)?)
}

#[cfg(not(unix))]
pub fn send(_req: &Request) -> Result<Response> {
    Ok(Response {
        ok: false,
        message: Some("IPC not supported on this platform".into()),
    })
}
