//! Codex Desktop's local named pipe, retaining the shared Desktop IPC framing.
//!
//! Source: ChatGPT 26.1016.61520, app.asar application-network-startup-BG13czUt.js
//! selects `\\.\pipe\codex-ipc` on win32; src-CsH31EEu.js uses the same router,
//! initialization and owner-discovery messages on both operating systems.
//! Peer validation follows OpenAI's codex-rs/tui/src/ide_context/windows_pipe.rs
//! at 9a8a85bc539325f2819dd0717f88b6ba6286495e:
//! query the connected pipe server's TokenUser, never infer identity from its name.
use super::{DesktopIpc, IpcError};
use std::net::Shutdown;
use tokio::sync::watch;
use std::{
    io::{self, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::windows::named_pipe::{ClientOptions, NamedPipeClient},
    runtime::Runtime,
};
use windows_sys::Win32::{
    Foundation::{ERROR_PIPE_BUSY, HANDLE},
    Security::{
        Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
        EqualSid, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER,
    },
    System::{
        Pipes::GetNamedPipeServerProcessId,
        Threading::{
            GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
};

pub fn endpoint(_data: &Path) -> PathBuf {
    PathBuf::from(r"\\.\pipe\codex-ipc")
}

pub(super) fn random(bytes: &mut [u8]) -> io::Result<()> {
    let length = u32::try_from(bytes.len()).map_err(|_| io::ErrorKind::InvalidInput)?;
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            length,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(io::Error::other("The Windows random source is unavailable"));
    }
    Ok(())
}

/// A same-user pipe with one deadline across connection, reads and writes.
// Local Windows port: wake blocked reads/writes when the observer stops.
// A persistent watch value also cancels operations started after shutdown.
pub struct ShutdownHandle(watch::Sender<bool>);
impl ShutdownHandle {
    pub fn shutdown(&self, _how: Shutdown) -> io::Result<()> {
        self.0.send_replace(true);
        Ok(())
    }
}
impl DesktopIpc {
    pub fn shutdown_handle(&self) -> Result<ShutdownHandle, IpcError> {
        Ok(ShutdownHandle(self.socket.cancel.clone()))
    }
}
pub struct LocalSocket {
    pipe: NamedPipeClient,
    prefetched: std::cell::Cell<Option<u8>>,
    cancel: watch::Sender<bool>,
    runtime: Runtime,
    deadline: Instant,
}

impl LocalSocket {
    pub fn connect(path: &Path, timeout: Duration) -> Result<Self, IpcError> {
        if timeout.is_zero() {
            return Err(IpcError::TimedOut);
        }
        if !path.to_string_lossy().starts_with(r"\\.\pipe\") {
            return Err(IpcError::Unavailable);
        }
        let deadline = Instant::now() + timeout;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let pipe = runtime.block_on(async {
            loop {
                // Tokio defaults to SECURITY_IDENTIFICATION | SECURITY_SQOS_PRESENT,
                // so the server cannot impersonate this client to access its resources.
                match ClientOptions::new().open(path) {
                    Ok(pipe) => return Ok(pipe),
                    Err(error) if error.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => {
                        let left = deadline.saturating_duration_since(Instant::now());
                        if left.is_zero() {
                            return Err(io::Error::from(io::ErrorKind::TimedOut));
                        }
                        tokio::time::sleep(left.min(Duration::from_millis(10))).await;
                    }
                    Err(error) => return Err(error),
                }
            }
        })?;
        validate_server(pipe.as_raw_handle())?;
        let (cancel, _) = watch::channel(false);
        Ok(Self {
            pipe,
            prefetched: std::cell::Cell::new(None),
            cancel,
            runtime,
            deadline,
        })
    }

    pub fn renew(&mut self, timeout: Duration) {
        self.deadline = Instant::now() + timeout;
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub fn wait_for_message(&self, timeout: Duration) -> Result<bool, IpcError> {
        let mut cancelled = self.cancel.subscribe();
        self.runtime.block_on(async {
            if *cancelled.borrow() { return Err(IpcError::Unavailable); }
            if self.prefetched.get().is_some() { return Ok(true); }
            tokio::select! {
                biased;
                _ = cancelled.changed() => Err(IpcError::Unavailable),
                result = tokio::time::timeout(timeout, async {
                    // Readiness can remain set after the previous frame was read.
                    // Probe one byte: WouldBlock clears Tokio readiness; keep a
                    // real byte for the framing reader so answers can still poll.
                    loop {
                        self.pipe.readable().await?;
                        let mut byte = [0];
                        match self.pipe.try_read(&mut byte) {
                            Ok(0) => return Err(io::Error::from(io::ErrorKind::UnexpectedEof)),
                            Ok(_) => { self.prefetched.set(Some(byte[0])); return Ok(()); }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                            Err(error) => return Err(error),
                        }
                    }
                }) => match result {
                Ok(result) => result.map(|_| true).map_err(IpcError::from),
                Err(_) => Ok(false),
                }
            }
        })
    }
}

impl Read for LocalSocket {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() { return Ok(0); }
        if *self.cancel.borrow() { return Err(io::ErrorKind::ConnectionAborted.into()); }
        if let Some(byte) = self.prefetched.take() { bytes[0] = byte; return Ok(1); }
        let remaining = self.remaining();
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut cancelled = self.cancel.subscribe();
        self.runtime.block_on(async {
            if *cancelled.borrow() { return Err(io::ErrorKind::ConnectionAborted.into()); }
            tokio::select! {
                biased;
                _ = cancelled.changed() => Err(io::ErrorKind::ConnectionAborted.into()),
                result = tokio::time::timeout(remaining, self.pipe.read(bytes)) =>
                    result.map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?,
            }
        })
    }
}

impl Write for LocalSocket {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let remaining = self.remaining();
        if remaining.is_zero() {
            return Err(io::ErrorKind::TimedOut.into());
        }
        let mut cancelled = self.cancel.subscribe();
        self.runtime.block_on(async {
            if *cancelled.borrow() { return Err(io::ErrorKind::ConnectionAborted.into()); }
            tokio::select! {
                biased;
                _ = cancelled.changed() => Err(io::ErrorKind::ConnectionAborted.into()),
                result = tokio::time::timeout(remaining, self.pipe.write(bytes)) =>
                    result.map_err(|_| io::Error::from(io::ErrorKind::TimedOut))?,
            }
        })
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn token_user(process: HANDLE) -> io::Result<Vec<usize>> {
    let mut token = std::ptr::null_mut();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut length,
        )
    };
    if (length as usize) < std::mem::size_of::<TOKEN_USER>() {
        return Err(io::Error::last_os_error());
    }
    let mut buffer = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            length,
            &mut length,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(buffer)
}

fn validate_server(pipe: HANDLE) -> io::Result<()> {
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(process) };
    let server_buffer = token_user(process.as_raw_handle())?;
    let current_buffer = token_user(unsafe { GetCurrentProcess() })?;
    let server = unsafe { &*server_buffer.as_ptr().cast::<TOKEN_USER>() };
    let current = unsafe { &*current_buffer.as_ptr().cast::<TOKEN_USER>() };
    if unsafe { EqualSid(server.User.Sid, current.User.Sid) } == 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "The Codex pipe server belongs to another user",
        ));
    }
    Ok(())
}

// Private send snapshots retain the shared bounded allocation limit. The Unix
// observer's large-frame mmap optimization remains in its own transport module.
pub(super) struct Frame(Vec<u8>);
impl Frame {
    pub(super) fn new(length: usize) -> Result<Self, IpcError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(length)
            .map_err(|_| IpcError::Unavailable)?;
        bytes.resize(length, 0);
        Ok(Self(bytes))
    }
    pub(super) fn bytes_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
impl std::ops::Deref for Frame {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

#[cfg(test)]
mod tests;
