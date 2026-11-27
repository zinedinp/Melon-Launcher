// Release builds on Windows are a GUI program, so no console window opens next to the launcher.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod authlib;
mod catalog;
mod cdn;
mod java_play;
mod lang;
mod launcher;
mod mods;
mod play;
mod store;
mod toolchain;
mod ui;

fn main() -> anyhow::Result<()> {
    let validation = std::env::args().any(|a| a == "--vulkan-validation");
    launcher::run(validation)
}
