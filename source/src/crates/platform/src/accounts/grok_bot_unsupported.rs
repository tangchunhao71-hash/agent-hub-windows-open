use super::super::AccountContext;
pub(super) fn activity(_: &AccountContext) -> Result<serde_json::Value,String> {
    Err(agent_hud_core::l10n::text("此系统暂未接入 Grok Bot 工作状态读取","Grok Bot activity is not connected on this operating system"))
}
pub(super) fn read(_: &AccountContext) -> Result<serde_json::Value,String> {
    Err(agent_hud_core::l10n::text("此系统暂未接入 Grok Bot 分 Bot 用量读取","Grok Bot per-Bot usage is not connected on this operating system"))
}
