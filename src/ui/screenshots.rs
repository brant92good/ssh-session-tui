//! Export the actual Ratatui buffer with isolated demonstration metadata.
use super::*;
use ratatui::{
    backend::TestBackend,
    style::{Color, Modifier},
};
use std::{fmt::Write as _, fs, path::Path};

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn color(value: Color, fallback: &str) -> String {
    match value {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Black => "#000000".into(),
        Color::White => "#ffffff".into(),
        _ => fallback.into(),
    }
}
fn save(picker: &Picker, output: &Path, title: &str) -> Result<()> {
    let (columns, rows) = (100u16, 26u16);
    let mut terminal = Terminal::new(TestBackend::new(columns, rows))?;
    terminal.draw(|frame| picker.draw(frame))?;
    let width = u32::from(columns) * 9 + 32;
    let height = u32::from(rows) * 19 + 32;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><title>{}</title><rect width=\"100%\" height=\"100%\" rx=\"12\" fill=\"#0f172a\"/><g font-family=\"'DejaVu Sans Mono', Consolas, monospace\" font-size=\"15\" xml:space=\"preserve\">",
        xml(title)
    );
    let buffer = terminal.backend().buffer();
    for y in 0..rows {
        let mut x = 0;
        while x < columns {
            let cell = &buffer[(x, y)];
            let begin = x;
            let mut text = String::new();
            while x < columns {
                let current = &buffer[(x, y)];
                if current.fg != cell.fg
                    || current.bg != cell.bg
                    || current.modifier != cell.modifier
                {
                    break;
                }
                text.push_str(current.symbol());
                x += 1;
            }
            let px = u32::from(begin) * 9 + 16;
            let py = u32::from(y) * 19 + 16;
            let run_width = u32::from(x - begin) * 9;
            write!(
                svg,
                "<rect x=\"{px}\" y=\"{py}\" width=\"{run_width}\" height=\"19\" fill=\"{}\"/>",
                color(cell.bg, "#0f172a")
            )?;
            if !text.trim().is_empty() {
                write!(
                    svg,
                    "<text x=\"{px}\" y=\"{}\" fill=\"{}\" font-weight=\"{}\" textLength=\"{run_width}\" lengthAdjust=\"spacingAndGlyphs\">{}</text>",
                    py + 15,
                    color(cell.fg, "#e2e8f0"),
                    if cell.modifier.contains(Modifier::BOLD) {
                        700
                    } else {
                        400
                    },
                    xml(&text)
                )?;
            }
        }
    }
    svg.push_str("</g></svg>\n");
    fs::write(output, svg)?;
    Ok(())
}
pub fn capture(output: &Path) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )?;
    let specs = [
        ("dev", "Development", "Work/Apps", "web"),
        ("gpu", "Training box", "Lab/GPU", "gpu"),
        ("build", "Build runner", "Work/CI", "linux"),
        ("staging", "Staging", "Work/Apps", "web"),
    ];
    let machines = specs
        .into_iter()
        .enumerate()
        .map(|(index, (id, name, group, tag))| Machine {
            id: id.into(),
            name: name.into(),
            user: "dev".into(),
            group: group.into(),
            tags: vec![tag.into()],
            routes: vec![
                Route {
                    id: "lan".into(),
                    name: "LAN".into(),
                    host: format!("192.0.2.{}", 10 + index),
                    port: 22,
                    ssh_alias: None,
                },
                Route {
                    id: "vpn".into(),
                    name: "Tailscale".into(),
                    host: format!("{id}.example.test"),
                    port: 22,
                    ssh_alias: None,
                },
            ],
        })
        .collect::<Vec<_>>();
    catalog.save(&machines, &catalog.load()?.revision)?;
    for machine in &machines {
        catalog.choose(machine, "lan")?;
    }
    Favorites::assign(&catalog, "1", Some("dev"), None)?;
    Favorites::assign(&catalog, "2", Some(LOCAL), None)?;
    Favorites::assign(&catalog, "3", Some("gpu"), None)?;
    let mut picker = Picker::new(catalog)?;
    picker.local_shell = "pwsh".into();
    picker.selected = 1;
    fs::create_dir_all(output)?;
    save(
        &picker,
        &output.join("picker.svg"),
        "SSH Sessions native picker with demonstration machines",
    )?;
    picker.screen = Screen::Groups { selected: 1 };
    save(
        &picker,
        &output.join("groups.svg"),
        "SSH Sessions native group browser",
    )?;
    picker.screen = Screen::Routes {
        machine: "dev".into(),
        selected: 1,
        fallback: false,
        connect_after: false,
    };
    save(
        &picker,
        &output.join("routes.svg"),
        "SSH Sessions native per-device route chooser",
    )?;
    let config = temp.path().join("demo-ssh-config");
    fs::write(
        &config,
        "Host development\n HostName 192.0.2.10\n User dev\nHost training\n HostName 192.0.2.11\n User dev\n",
    )?;
    let mut scan = ssh_import::scan(Some(&config))?;
    // The screen is a static export; do not publish the machine's temporary path.
    scan.config = Path::new("~/.ssh/config").to_path_buf();
    picker.screen = Screen::Import {
        scan,
        selected: 0,
        marked: BTreeSet::new(),
        group: "Work".into(),
        revision: picker.snapshot.revision.clone(),
    };
    save(
        &picker,
        &output.join("import.svg"),
        "SSH Sessions native SSH import preview with demonstration config",
    )?;
    Ok(())
}
