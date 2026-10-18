use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::os::windows::io::FromRawHandle;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_GENERIC_READ, FILE_GENERIC_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

const PIPE_ACCESS_DUPLEX: FILE_FLAGS_AND_ATTRIBUTES = FILE_FLAGS_AND_ATTRIBUTES(0x00000003);

pub const PIPE_NAME: &str = r"\\.\pipe\wallmus-ipc-v1";

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcRequest {
    pub v: u32,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcResponse {
    pub v: u32,
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct IpcEvent {
    pub v: u32,
    pub event: String,
    pub data: serde_json::Value,
}

pub trait IpcHandler: Send + Sync + 'static {
    fn handle_method(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value>;
}

pub struct IpcServer {
    running: Arc<AtomicBool>,
}

impl IpcServer {
    pub fn start<H: IpcHandler>(handler: Arc<H>) -> Result<Self> {
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        thread::Builder::new()
            .name("ipc-server".to_string())
            .spawn(move || {
                run_server_loop(handler, running_clone);
            })
            .context("Failed to spawn IPC server thread")?;

        Ok(Self { running })
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        // Wakeup pipe ConnectNamedPipe by opening and immediately closing handle
        let wide_pipe: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            if let Ok(h) = CreateFileW(
                windows::core::PCWSTR(wide_pipe.as_ptr()),
                FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
                windows::Win32::Storage::FileSystem::FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                HANDLE::default(),
            ) {
                if !h.is_invalid() {
                    let _ = CloseHandle(h);
                }
            }
        }
    }
}

fn run_server_loop<H: IpcHandler>(handler: Arc<H>, running: Arc<AtomicBool>) {
    let wide_pipe: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

    while running.load(Ordering::SeqCst) {
        unsafe {
            let pipe_handle = CreateNamedPipeW(
                PCWSTR(wide_pipe.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                None,
            );

            if pipe_handle == INVALID_HANDLE_VALUE {
                thread::sleep(std::time::Duration::from_millis(500));
                continue;
            }

            let connected = ConnectNamedPipe(pipe_handle, None).is_ok();
            if connected && running.load(Ordering::SeqCst) {
                handle_client_connection(pipe_handle, &*handler);
            }

            let _ = DisconnectNamedPipe(pipe_handle);
            let _ = CloseHandle(pipe_handle);
        }
    }
}

fn handle_client_connection<H: IpcHandler>(pipe: HANDLE, handler: &H) {
    let file = unsafe { std::fs::File::from_raw_handle(pipe.0 as _) };
    let mut reader = BufReader::new(&file);
    let mut writer = &file;

    let mut line = String::new();
    while let Ok(bytes) = reader.read_line(&mut line) {
        if bytes == 0 {
            break;
        }

        let resp = process_line(&line, handler);
        if let Ok(json_str) = serde_json::to_string(&resp) {
            let _ = writer.write_all(json_str.as_bytes());
            let _ = writer.write_all(b"\n");
            let _ = writer.flush();
        }
        line.clear();
    }

    // Forget file so raw handle is not double-freed (handled by CloseHandle)
    std::mem::forget(file);
}

fn process_line<H: IpcHandler>(line: &str, handler: &H) -> IpcResponse {
    let req: IpcRequest = match serde_json::from_str(line.trim()) {
        Ok(r) => r,
        Err(e) => {
            return IpcResponse {
                v: 1,
                id: 0,
                ok: false,
                result: None,
                error: Some(IpcError {
                    code: "invalid_request".to_string(),
                    message: e.to_string(),
                }),
            };
        }
    };

    if req.v != 1 {
        return IpcResponse {
            v: 1,
            id: req.id,
            ok: false,
            result: None,
            error: Some(IpcError {
                code: "version_mismatch".to_string(),
                message: format!("Unsupported protocol version: {}", req.v),
            }),
        };
    }

    match handler.handle_method(&req.method, req.params) {
        Ok(res) => IpcResponse {
            v: 1,
            id: req.id,
            ok: true,
            result: Some(res),
            error: None,
        },
        Err(err) => IpcResponse {
            v: 1,
            id: req.id,
            ok: false,
            result: None,
            error: Some(IpcError {
                code: "method_error".to_string(),
                message: err.to_string(),
            }),
        },
    }
}

/// Send request to running wallmus daemon over Named Pipe.
pub fn send_request(method: &str, params: serde_json::Value) -> Result<IpcResponse> {
    use std::os::windows::io::FromRawHandle;

    let wide_pipe: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let handle = match CreateFileW(
            PCWSTR(wide_pipe.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            windows::Win32::Storage::FileSystem::FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE::default(),
        ) {
            Ok(h) if !h.is_invalid() => h,
            _ => bail!("Could not connect to wallmus IPC pipe (daemon might not be running)"),
        };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_named_pipe() {
        let wide_pipe: Vec<u16> = PIPE_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let handle = CreateNamedPipeW(
                PCWSTR(wide_pipe.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                None,
            );
            assert_ne!(handle, INVALID_HANDLE_VALUE, "GetLastError: {:?}", windows::Win32::Foundation::GetLastError());
            let _ = CloseHandle(handle);
        }
    }
}

