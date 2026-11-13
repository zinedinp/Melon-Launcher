mod cdn;
mod java_play;
mod launcher;
mod ui;

fn main() -> anyhow::Result<()> {
    let validation = std::env::args().any(|a| a == "--vulkan-validation");
    launcher::run(validation)
}
