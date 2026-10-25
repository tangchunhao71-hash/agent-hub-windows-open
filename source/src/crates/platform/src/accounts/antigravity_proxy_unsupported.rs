//! Other hosts retain their existing CLI child environment and discovery.
use super::AccountContext;
use agent_hud_agents::account::EngineStart;
use std::{collections::HashMap, path::{Path, PathBuf}};

pub(super) fn installed_cli_root(_home: &Path, _env: &HashMap<String, String>) -> Option<PathBuf> { None }

pub(super) fn environment<'a>(_context: &AccountContext, start: &EngineStart<'a>) -> Result<Vec<(&'a str, String)>, String> {
    Ok(start.env.to_vec())
}
