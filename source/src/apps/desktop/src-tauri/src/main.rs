#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The independent release observes clients without taking over the legacy
    // product's hooks or plugin registration. Owner migration is a later feature.
    let canonical_executable = None;
    let collaboration = agent_hud_host::collaboration::Collaboration::from_env()
        .unwrap_or_else(|error| { eprintln!("Collaboration disabled: {error}"); None });
    if let Some(product) = &collaboration { product.run_worker(); }
    agent_hud_host::run_with_options(
        tauri::generate_context!(),
        collaboration.map(|product| product as std::sync::Arc<dyn agent_hud_host::extension::ProductExtension>),
        agent_hud_host::HostOptions {
            canonical_executable,
        },
    );
}
