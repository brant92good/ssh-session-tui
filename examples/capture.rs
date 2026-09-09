fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "docs/screenshots".into());
    ssh_sessions::ui::capture(std::path::Path::new(&output))
}
