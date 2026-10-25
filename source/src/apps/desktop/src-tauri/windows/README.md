# Agent Bot installer

`installer.nsi` derives from the Tauri CLI **v2.12.3** template:
https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.12.3/crates/tauri-bundler/src/bundle/windows/nsis/installer.nsi

Original SHA-256: `8ae02c66f8adb4a0d15d97f984fbacd2b44262d8a4d03fefaf6177f67f7008e5`.
License: Apache-2.0 OR MIT, copyright Tauri Programme within The Commons Conservancy.

Changes: call AgentBotPreflight before the upstream initializer; restore WebView2 before any upgrade removal and in the WebView2 section; append the three AgentBot functions. Keep upstream current-user registry, upgrade and uninstall behavior. Preflight uses GetNativeSystemInfo and RtlGetVersion (Windows 11 client, AMD64), before any removal or dependency work. WebView2 uses the official Microsoft bootstrapper in a private temporary directory, checks machine/user registration even during upgrades, and offers retry or the official page on failure. Silent mode fails with a nonzero code without opening a browser. A dependency reboot requirement exits with 3010 and asks the user to rerun after reboot.

When updating Tauri, rebase these changes onto its matching template and compile/test again. Do not change product ID, install registry keys or delete app data as an upgrade shortcut.
