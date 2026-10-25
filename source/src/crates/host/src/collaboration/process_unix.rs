use std::process::{Child, Command};

pub(super) fn hide(_command: &mut Command) {}
pub(super) fn stop_tree(_child: &mut Child) {}
