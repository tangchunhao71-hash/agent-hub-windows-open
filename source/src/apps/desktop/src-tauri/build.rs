fn main() {
    if std::env::var("PROFILE").as_deref() == Ok("release")
        && std::env::var("CARGO_FEATURE_CUSTOM_PROTOCOL").as_deref() != Ok("1")
    {
        panic!("Release desktop builds require custom-protocol to embed the frontend; use scripts/build-windows.ps1");
    }
    tauri_build::build()
}
