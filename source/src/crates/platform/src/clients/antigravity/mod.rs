// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Antigravity's native channel: its running language server
//! (`agents::antigravity::LOCAL_SERVICE`), for quota and for the permission
//! requests it waits on.
pub mod activity;
pub(crate) mod permissions;
pub mod service;
