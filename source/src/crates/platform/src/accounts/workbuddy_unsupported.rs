use super::super::AccountContext;
use serde_json::Value;

pub(super) fn read(_context: &AccountContext) -> Result<Value, String> {
    Err(agent_hud_core::l10n::text(
        "此系统尚未接通 WorkBuddy 免费额度读取",
        "WorkBuddy free quota acquisition is not implemented on this operating system",
    ))
}
