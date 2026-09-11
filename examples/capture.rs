fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "docs/screenshots".into());
    let output = std::path::Path::new(&output);
    ssh_sessions::ui::capture(output)?;
    ssh_sessions::files_picker::capture(output)
}
