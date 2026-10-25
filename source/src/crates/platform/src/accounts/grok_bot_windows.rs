//! Grok Bot's own Windows login, read-only; never requires Cursor.
use super::super::{engine, AccountContext};
use agent_hud_core::l10n;
use serde_json::Value;
use std::{path::PathBuf, sync::atomic::Ordering, time::{Duration, Instant}};

const SCRIPT: &str = include_str!("../../scripts/windows/grok-bot-usage.cjs");
fn response(value: Value) -> Result<Value,String> {
    if value["success"]==true{return Ok(value);}
    Err(match value["errorCategory"].as_str(){
        Some("missing-login"|"missing-account"|"expired-login"|"http-401")=>l10n::text("Grok Bot 登录不可用，暂不能读取分 Bot 用量","Grok Bot login is unavailable; per-Bot usage cannot be read"),
        Some("account-changed"|"account-mismatch")=>l10n::text("Grok Bot 账户已变化，请重试用量读取","Grok Bot account changed; retry the usage reading"),
        Some("unsupported-storage"|"secure-storage-unavailable")=>l10n::text("当前 Grok Bot 安全存储无法读取","The current Grok Bot secure storage is unavailable"),
        _=>l10n::text("Grok Bot 分 Bot 用量暂时无法读取","Grok Bot per-Bot usage is temporarily unavailable"),
    })
}
pub(super) fn read(context:&AccountContext)->Result<Value,String>{
    read_mode(context,"usage")
}
pub(super) fn activity(context:&AccountContext)->Result<Value,String>{
    read_mode(context,"activity")
}
fn read_mode(context:&AccountContext,mode:&str)->Result<Value,String>{
    let mut candidates=Vec::<PathBuf>::new();
    for key in ["ProgramFiles","ProgramW6432"] {
        if let Some(dir)=crate::os::variable(&context.env,key,&context.home){candidates.push(dir.join("nodejs/node.exe"));}
    }
    candidates.extend(std::env::split_paths(&crate::os::engine_path(&context.home,&context.env)).map(|dir|dir.join("node.exe")));
    let node=candidates.into_iter().find(|p|p.is_absolute()&&p.is_file()).ok_or_else(||l10n::text("读取 Grok Bot 用量需要本机 Node.js 运行时","Reading Grok Bot usage requires a local Node.js runtime"))?;
    let worker=engine::Engine::start(context,&node,&["--eval",SCRIPT,mode],&context.data_dir.join(if mode=="activity" {"account-engines/grok-bot-activity"}else{"account-engines/grok-bot"}),&[("NODE_OPTIONS",String::new()),("NODE_PATH",String::new())])?;
    let deadline=Instant::now()+Duration::from_secs(40);
    loop{
        if context.cancelled.load(Ordering::Acquire){return Err(l10n::text("账户读取已停止","Account reading was stopped"));}
        if let Some(value)=worker.next(deadline.min(Instant::now()+Duration::from_millis(100))){return response(value);}
        if worker.ended()||Instant::now()>=deadline{return Err(l10n::text("Grok Bot 用量读取未完成","Grok Bot usage reading did not complete"));}
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]
    fn grok_bot_errors_never_relay_credential_payloads(){
        let result=response(serde_json::json!({"success":false,"errorCategory":"fixture-secret"})).unwrap_err();
        assert!(!result.contains("fixture-secret"));
        assert!(response(serde_json::json!({"accountSlot":"fixture"})).is_err());
        assert_ne!(result,response(serde_json::json!({"errorCategory":"missing-login"})).unwrap_err());
    }
}
