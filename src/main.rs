mod catalog;
mod cdn;
mod java_play;
mod lang;
mod launcher;
mod mods;
mod play;
mod store;
mod ui;

fn main() -> anyhow::Result<()> {
    let validation = std::env::args().any(|a| a == "--vulkan-validation");
    launcher::run(validation)
}
