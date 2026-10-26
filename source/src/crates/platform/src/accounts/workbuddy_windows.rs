// Agent Bot modifications, 2026-10-26; upstream portions retain their original licenses.
//! Run the installed client's own read-only credential API in its Node mode.
//! No application boot, login, refresh, quota claim or credential file write.
use super::super::{engine, AccountContext};
use agent_hud_core::l10n;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const SCRIPT: &str = include_str!("../../scripts/windows/workbuddy-account.cjs");

fn response(value: Value) -> Result<Value, String> {
    if value["success"] == true {
        return Ok(value);
    }
    let message = match value["errorCategory"].as_str() {
        Some("logged-out" | "missing-account" | "http-401") => l10n::text(
            "请在 WorkBuddy 中登录后读取免费额度",
            "Sign in to WorkBuddy to read its free quota",
        ),
        Some("account-changed") => l10n::text(
            "读取期间 WorkBuddy 账户发生变化，请重试",
            "The WorkBuddy account changed during the reading; try again",
        ),
        Some("enterprise-account-not-free-personal") => l10n::text(
            "当前读取支持 WorkBuddy 个人免费额度，请切换到个人账户",
            "This reader supports personal WorkBuddy free quota; switch to a personal account",
        ),
        Some("key-schema" | "key-mismatch" | "unsupported-envelope" | "invalid-envelope") => {
            l10n::text(
                "当前 WorkBuddy 登录保护格式暂不兼容，需要更新读取适配",
                "This WorkBuddy credential format needs an updated reader",
            )
        }
        _ => l10n::text(
            "WorkBuddy 免费额度读取未完成，请确认账户页可访问后重试",
            "WorkBuddy free quota reading did not complete; check the account page and try again",
        ),
    };
    Err(message)
}

fn executable(context: &AccountContext) -> Option<PathBuf> {
    let local = crate::os::variable(&context.env, "LOCALAPPDATA", &context.home)
        .unwrap_or_else(|| context.home.join("AppData/Local"));
    let mut paths = vec![local.join("Programs/WorkBuddy/WorkBuddy.exe")];
    for key in ["ProgramFiles", "ProgramW6432"] {
        if let Some(folder) = crate::os::variable(&context.env, key, &context.home) {
            paths.push(folder.join("WorkBuddy/WorkBuddy.exe"));
        }
    }
    paths.into_iter().find(|path| path.is_file())
}

pub(super) fn read(context: &AccountContext) -> Result<Value, String> {
    let executable = executable(context).ok_or_else(|| {
        l10n::text(
            "未找到已安装的 WorkBuddy，无法读取免费额度",
            "Installed WorkBuddy was not found; its free quota cannot be read",
        )
    })?;
    let worker = engine::Engine::start(
        context,
        &executable,
        &["--eval", SCRIPT],
        &context.data_dir.join("account-engines/workbuddy"),
        &[
            ("ELECTRON_RUN_AS_NODE", "1".into()),
            ("NODE_OPTIONS", String::new()),
            ("NODE_PATH", String::new()),
        ],
    )?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if context.cancelled.load(Ordering::Acquire) {
            return Err(l10n::text("账户读取已停止", "Account reading was stopped"));
        }
        if let Some(value) = worker.next(deadline.min(Instant::now() + Duration::from_millis(100)))
        {
            return response(value);
        }
        if worker.ended() || Instant::now() >= deadline {
            return Err(l10n::text(
                "WorkBuddy 免费额度读取未完成，请稍后重试",
                "WorkBuddy free quota reading did not complete; try again later",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workbuddy_failure_does_not_relay_payloads_or_misreport_login() {
        let failure = response(serde_json::json!({"success": false,
            "errorCategory": "raw-auth-fixture-secret"}))
        .unwrap_err();
        assert!(!failure.contains("fixture-secret"));
        assert_ne!(
            failure,
            response(serde_json::json!({
                "success": false, "errorCategory": "logged-out"
            }))
            .unwrap_err()
        );
        assert!(response(serde_json::json!({"account": {}})).is_err());
    }
}
