//! IPC protocol definitions and Named Pipe client for wallmus.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING,
};

pub const PIPE_NAME: &str = r"\\.\pipe\wallmus-ipc-v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcRequest {
    pub v: u32,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcResponse {
    pub v: u32,
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

/// Send request to running wallmus daemon over Named Pipe.
pub fn send_request(method: &str, params: serde_json::Value) -> Result<IpcResponse> {
    use std::os::windows::io::FromRawHandle;

    let wide_pipe: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let mut handle = HANDLE::default();
        let mut connected = false;
        for _ in 0..40 {
            if let Ok(h) = CreateFileW(
                PCWSTR(wide_pipe.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                windows::Win32::Storage::FileSystem::FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE::default(),
            ) {
                if !h.is_invalid() {
                    handle = h;
                    connected = true;
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }

        if !connected {
            bail!("Could not connect to wallmus IPC pipe (daemon might not be running)");
        }

        let file = std::fs::File::from_raw_handle(handle.0 as _);
        let mut writer = &file;
        let mut reader = BufReader::new(&file);

        let req = IpcRequest {
            v: 1,
            id: 1,
            method: method.to_string(),
            params,
        };

        let req_json = serde_json::to_string(&req)?;
        writer.write_all(req_json.as_bytes())?;
        writer.write_all(b"\n")?;
        writer.flush()?;

        let mut resp_line = String::new();
        reader.read_line(&mut resp_line)?;

        let resp: IpcResponse = serde_json::from_str(resp_line.trim())?;
        Ok(resp)
    }
}
