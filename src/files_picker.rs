//! Files entry point: browse metadata first, freeze a route only on explicit open.
#[cfg(feature = "screenshots")]
mod screenshots;
use crate::{
    catalog::{Catalog, Machine, Snapshot},
    connection,
    file_presets::{self, FilePresets, Preset},
    files, organization,
    text::casefold,
    ui::{Input, TerminalGuard},
};
use anyhow::{Context, Result, ensure};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
#[cfg(feature = "screenshots")]
pub use screenshots::capture;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum RowKey {
    Group(String),
    Machine(String),
    Path(String, String),
}
struct Row {
    key: RowKey,
    text: String,
}
#[derive(Clone)]
struct Target {
    machine: String,
    preset: Option<String>,
}
#[derive(Clone)]
enum Modal {
    Search(Input),
    Edit {
        machine: String,
        id: String,
        fields: [Input; 2],
        selected: usize,
    },
    Delete {
        machine: String,
        id: String,
        name: String,
    },
    Routes {
        target: Target,
        cursor: usize,
    },
    Saved,
    Help,
}
struct Picker {
    catalog: Catalog,
    snapshot: Snapshot,
    presets: Option<(FilePresets, String)>,
    preset_error: String,
    closed: BTreeSet<String>,
    expanded: BTreeSet<String>,
    query: String,
    rows: Vec<Row>,
    selected: Option<usize>,
    modal: Option<Modal>,
    notice: String,
}
impl Picker {
    fn new(catalog: Catalog) -> Result<Self> {
        let snapshot = catalog.load()?;
        let (presets, preset_error) = match FilePresets::load(&catalog) {
            Ok(value) => (Some(value), String::new()),
            Err(error) => (
                None,
                format!("Saved paths unavailable: {error:#}. Server home still works."),
            ),
        };
        let mut picker = Self {
            catalog,
            snapshot,
            presets,
            preset_error,
            closed: BTreeSet::new(),
            expanded: BTreeSet::new(),
            query: String::new(),
            rows: Vec::new(),
            selected: None,
            modal: None,
            notice: String::new(),
        };
        picker.rebuild(false);
        Ok(picker)
    }
    fn paths(&self, machine: &str) -> &[Preset] {
        self.presets
            .as_ref()
            .map(|(p, _)| p.for_machine(machine))
            .unwrap_or(&[])
    }
    fn selected_key(&self) -> Option<RowKey> {
        self.selected
            .and_then(|i| self.rows.get(i))
            .map(|row| row.key.clone())
    }
    fn select_key(&mut self, key: &RowKey) {
        self.selected = self.rows.iter().position(|row| &row.key == key);
    }
    fn rebuild(&mut self, keep_selection: bool) {
        let previous = self.selected_key();
        let mut groups: BTreeMap<String, (String, Vec<&Machine>)> = BTreeMap::new();
        let query = casefold(&self.query);
        for machine in &self.snapshot.machines {
            if !organization::matches(machine, &self.query)
                && !self
                    .paths(&machine.id)
                    .iter()
                    .any(|p| casefold(&format!("{} {}", p.name, p.path)).contains(&query))
            {
                continue;
            }
            let key = casefold(&machine.group);
            groups
                .entry(key)
                .or_insert_with(|| (machine.group.clone(), Vec::new()))
                .1
                .push(machine);
        }
        let mut rows = Vec::new();
        for (key, (name, mut machines)) in groups {
            machines.sort_by_key(|m| (casefold(&m.name), m.id.clone()));
            let open = !self.closed.contains(&key) || !self.query.is_empty();
            rows.push(Row {
                key: RowKey::Group(key),
                text: format!(
                    "{} {} ({})",
                    if open { "▾" } else { "▸" },
                    if name.is_empty() { "Ungrouped" } else { &name },
                    machines.len()
                ),
            });
            if !open {
                continue;
            }
            for machine in machines {
                let open = self.expanded.contains(&machine.id) || !self.query.is_empty();
                rows.push(Row {
                    key: RowKey::Machine(machine.id.clone()),
                    text: format!("  {} {}", if open { "▾" } else { "▸" }, machine.name),
                });
                if open {
                    for preset in self.paths(&machine.id) {
                        if !self.query.is_empty()
                            && !organization::matches(machine, &self.query)
                            && !casefold(&format!("{} {}", preset.name, preset.path))
                                .contains(&query)
                        {
                            continue;
                        }
                        rows.push(Row {
                            key: RowKey::Path(machine.id.clone(), preset.id.clone()),
                            text: format!("      {}  ·  {}", preset.name, preset.path),
                        });
                    }
                }
            }
        }
        self.rows = rows;
        if keep_selection {
            self.selected =
                previous.and_then(|key| self.rows.iter().position(|row| row.key == key));
        } else {
            self.selected = (!self.rows.is_empty()).then_some(0);
        }
    }
    fn reload(&mut self) -> Result<()> {
        self.snapshot = self.catalog.load()?;
        match FilePresets::load(&self.catalog) {
            Ok(value) => {
                self.presets = Some(value);
                self.preset_error.clear();
            }
            Err(error) => {
                self.presets = None;
                self.preset_error =
                    format!("Saved paths unavailable: {error:#}. Server home still works.");
            }
        }
        self.rebuild(true);
        self.notice = if self.selected.is_none() {
            "Selection changed. Choose a row again."
        } else {
            "Catalog and saved paths reloaded."
        }
        .into();
        Ok(())
    }
    fn machine(&self, id: &str) -> Result<&Machine> {
        self.snapshot
            .machines
            .iter()
            .find(|m| m.id == id)
            .context("Machine changed; press F5 and select again.")
    }
    fn target(&self) -> Result<Target> {
        match self.selected_key() {
            Some(RowKey::Machine(machine)) => Ok(Target {
                machine,
                preset: None,
            }),
            Some(RowKey::Path(machine, preset)) => Ok(Target {
                machine,
                preset: Some(preset),
            }),
            _ => anyhow::bail!("Select a server or one of its saved paths first."),
        }
    }
    fn launch(&mut self, target: Target, route_id: Option<&str>) -> Result<Option<files::Launch>> {
        let machine = self.machine(&target.machine)?;
        let route = if let Some(id) = route_id {
            machine.routes.iter().find(|r| r.id == id)
        } else {
            let preferences = self.catalog.preferences()?;
            self.catalog.preferred(machine, &preferences)
        };
        let Some(route) = route else {
            self.modal = Some(Modal::Routes { target, cursor: 0 });
            self.notice = "Choose a route to use once. This does not change your default.".into();
            return Ok(None);
        };
        let path = if let Some(id) = &target.preset {
            let (presets, revision) = self
                .presets
                .as_ref()
                .context("Saved paths unavailable. Reload or choose server home.")?;
            file_presets::validate_snapshot(&self.catalog, &self.snapshot.revision, revision)?;
            Some(
                presets
                    .get(&target.machine, id)
                    .context("Saved path changed. Reload and select again.")?
                    .path
                    .as_str(),
            )
        } else {
            None
        };
        let launch = files::prepare_at(
            &self.catalog,
            &self.snapshot.revision,
            machine,
            route,
            &files::executable()?,
            &std::env::current_dir()?,
            path,
        )?;
        if target.preset.is_some() {
            file_presets::validate_snapshot(
                &self.catalog,
                &self.snapshot.revision,
                &self.presets.as_ref().unwrap().1,
            )?;
        }
        Ok(Some(launch))
    }
    fn edit(&mut self, existing: bool) -> Result<()> {
        let target = self.target()?;
        let (presets, _) = self
            .presets
            .as_ref()
            .context("Saved paths unavailable. Fix or restore the path file before editing.")?;
        let (id, name, path) = if existing {
            let id = target.preset.context("Select a saved path to edit.")?;
            let preset = presets
                .get(&target.machine, &id)
                .context("Saved path changed; reload.")?;
            (id, preset.name.clone(), preset.path.clone())
        } else {
            (crate::catalog::id(), String::new(), ".".into())
        };
        self.modal = Some(Modal::Edit {
            machine: target.machine,
            id,
            fields: [Input::new(name), Input::new(path)],
            selected: 0,
        });
        Ok(())
    }
    fn move_by(&mut self, amount: isize) {
        if !self.rows.is_empty() {
            self.selected = Some(
                self.selected
                    .unwrap_or(0)
                    .saturating_add_signed(amount)
                    .min(self.rows.len() - 1),
            );
        }
    }
    fn key(&mut self, key: KeyEvent) -> Result<Action> {
        if key.kind == KeyEventKind::Release {
            return Ok(Action::Continue);
        }
        if key.kind == KeyEventKind::Repeat
            && !matches!(
                key.code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::Left
                    | KeyCode::Right
                    | KeyCode::Backspace
                    | KeyCode::Char(_)
            )
        {
            return Ok(Action::Continue);
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'q'))
        {
            return Ok(Action::Quit);
        }
        if let Some(mut modal) = self.modal.take() {
            if key.code == KeyCode::Esc {
                return Ok(Action::Continue);
            }
            let restore = modal.clone();
            let result: Result<Option<files::Launch>> = (|| {
                match &mut modal {
                    Modal::Search(input) => {
                        if key.code == KeyCode::Enter {
                            self.query = input.text.clone();
                            self.rebuild(false);
                            return Ok(None);
                        }
                        input_key(input, key, 512);
                    }
                    Modal::Edit {
                        machine,
                        id,
                        fields,
                        selected,
                    } => match key.code {
                        KeyCode::Tab | KeyCode::BackTab => *selected = 1 - *selected,
                        KeyCode::Enter => {
                            let (presets, revision) =
                                self.presets.as_mut().context("Saved paths unavailable.")?;
                            *revision = presets.upsert(
                                &self.catalog,
                                machine,
                                Preset {
                                    id: id.clone(),
                                    name: fields[0].text.clone(),
                                    path: fields[1].text.clone(),
                                },
                                &self.snapshot.revision,
                                revision,
                            )?;
                            let key = RowKey::Path(machine.clone(), id.clone());
                            self.expanded.insert(machine.clone());
                            self.rebuild(true);
                            self.select_key(&key);
                            self.notice = "Saved path added or updated.".into();
                            // Windows terminal paste can arrive as ordinary key events.
                            // A trailing pasted newline must not become Browse/Enter.
                            self.modal = Some(Modal::Saved);
                            return Ok(None);
                        }
                        _ => input_key(
                            &mut fields[*selected],
                            key,
                            if *selected == 0 { 320 } else { 4096 },
                        ),
                    },
                    Modal::Delete { machine, id, .. }
                        if key.code == KeyCode::Delete && key.kind == KeyEventKind::Press =>
                    {
                        let (presets, revision) =
                            self.presets.as_mut().context("Saved paths unavailable.")?;
                        *revision = presets.delete(
                            &self.catalog,
                            machine,
                            id,
                            &self.snapshot.revision,
                            revision,
                        )?;
                        self.rebuild(true);
                        self.notice = "Saved path removed. Server files were not changed.".into();
                        return Ok(None);
                    }
                    Modal::Routes { target, cursor } => {
                        let machine = self.machine(&target.machine)?;
                        match key.code {
                            KeyCode::Up => *cursor = cursor.saturating_sub(1),
                            KeyCode::Down => {
                                *cursor = (*cursor + 1).min(machine.routes.len().saturating_sub(1))
                            }
                            KeyCode::Enter => {
                                let route = machine
                                    .routes
                                    .get(*cursor)
                                    .context("Select a route.")?
                                    .id
                                    .clone();
                                return self.launch(target.clone(), Some(&route));
                            }
                            _ => {}
                        }
                    }
                    Modal::Help if key.code == KeyCode::F(1) => return Ok(None),
                    _ => {}
                }
                self.modal = Some(modal);
                Ok(None)
            })();
            match result {
                Ok(Some(launch)) => return Ok(Action::Launch(launch)),
                Ok(None) => return Ok(Action::Continue),
                Err(error) => {
                    // Keep typed fields and selected route after validation/CAS failure.
                    if self.modal.is_none() {
                        self.modal = Some(restore);
                    }
                    return Err(error);
                }
            }
        }
        match key.code {
            KeyCode::Esc if !self.query.is_empty() => {
                self.query.clear();
                self.rebuild(false);
            }
            KeyCode::Esc | KeyCode::F(10) => return Ok(Action::Quit),
            KeyCode::Up => self.move_by(-1),
            KeyCode::Down => self.move_by(1),
            KeyCode::PageUp => self.move_by(-10),
            KeyCode::PageDown => self.move_by(10),
            KeyCode::Home => self.selected = (!self.rows.is_empty()).then_some(0),
            KeyCode::End => self.selected = self.rows.len().checked_sub(1),
            KeyCode::F(5) => self.reload()?,
            KeyCode::Char('/') => self.modal = Some(Modal::Search(Input::new(&self.query))),
            KeyCode::F(1) => self.modal = Some(Modal::Help),
            KeyCode::Char('a') if key.kind == KeyEventKind::Press => self.edit(false)?,
            KeyCode::Char('e') if key.kind == KeyEventKind::Press => self.edit(true)?,
            KeyCode::Char('r') if key.kind == KeyEventKind::Press => {
                self.modal = Some(Modal::Routes {
                    target: self.target()?,
                    cursor: 0,
                })
            }
            KeyCode::Delete if key.kind == KeyEventKind::Press => {
                let target = self.target()?;
                let id = target.preset.context("Select a saved path to remove.")?;
                let name = self
                    .presets
                    .as_ref()
                    .and_then(|(p, _)| p.get(&target.machine, &id))
                    .context("Saved paths unavailable.")?
                    .name
                    .clone();
                self.modal = Some(Modal::Delete {
                    machine: target.machine,
                    id,
                    name,
                });
            }
            KeyCode::Enter | KeyCode::Left | KeyCode::Right => match self.selected_key() {
                Some(RowKey::Group(group)) => {
                    match key.code {
                        KeyCode::Left => {
                            self.closed.insert(group);
                        }
                        KeyCode::Right => {
                            self.closed.remove(&group);
                        }
                        _ => {
                            if !self.closed.remove(&group) {
                                self.closed.insert(group);
                            }
                        }
                    }
                    self.rebuild(true);
                }
                Some(RowKey::Machine(id)) if key.code == KeyCode::Right => {
                    self.expanded.insert(id);
                    self.rebuild(true);
                }
                Some(RowKey::Machine(id)) if key.code == KeyCode::Left => {
                    self.expanded.remove(&id);
                    self.rebuild(true);
                }
                Some(RowKey::Path(machine, _)) if key.code == KeyCode::Left => {
                    self.select_key(&RowKey::Machine(machine));
                }
                Some(RowKey::Machine(_) | RowKey::Path(_, _)) if key.code == KeyCode::Enter => {
                    return self
                        .launch(self.target()?, None)
                        .map(|launch| launch.map(Action::Launch).unwrap_or(Action::Continue));
                }
                _ => {}
            },
            _ => {}
        }
        Ok(Action::Continue)
    }
}

fn input_key(input: &mut Input, key: KeyEvent, limit: usize) {
    if let KeyCode::Char(c) = key.code
        && !key
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        && (c.is_control() || input.text.len() + c.len_utf8() > limit)
    {
        return;
    }
    input.key(key);
}
enum Action {
    Continue,
    Quit,
    Launch(files::Launch),
}
const ACCENT: Color = Color::Rgb(101, 214, 190);
const MUTED: Color = Color::Rgb(148, 163, 184);
fn popup(frame: &mut ratatui::Frame, title: &str) -> Rect {
    let outer = frame.area();
    let width = outer.width.saturating_sub(4).min(100);
    let height = outer.height.saturating_sub(4).min(18);
    let area = Rect::new(
        outer.x + (outer.width - width) / 2,
        outer.y + (outer.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}
impl Picker {
    fn draw(&self, frame: &mut ratatui::Frame) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(
                Style::default()
                    .bg(Color::Rgb(15, 23, 42))
                    .fg(Color::Rgb(226, 232, 240)),
            ),
            area,
        );
        let parts = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(4),
            Constraint::Length(2),
        ])
        .split(area);
        frame.render_widget(Paragraph::new(format!("SSH Files · Choose a server\nEnter opens server home or a saved path. Right expands paths.\nSearch: {}", self.query)).style(Style::default().fg(ACCENT)), parts[0]);
        let items: Vec<_> = self
            .rows
            .iter()
            .map(|row| {
                ListItem::new(row.text.clone()).style(Style::default().fg(
                    if matches!(row.key, RowKey::Group(_)) {
                        ACCENT
                    } else {
                        Color::White
                    },
                ))
            })
            .collect();
        if items.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No matching servers.\nAdd or import machines in SSH Sessions, then press F5.",
                )
                .wrap(Wrap { trim: false }),
                parts[1],
            );
        } else {
            let mut state = ListState::default().with_selected(self.selected);
            frame.render_stateful_widget(
                List::new(items)
                    .highlight_style(
                        Style::default()
                            .bg(Color::Rgb(30, 61, 71))
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol("› "),
                parts[1],
                &mut state,
            );
        }
        frame.render_widget(
            Paragraph::new(format!("{}\n{}", self.notice, self.preset_error))
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(MUTED)),
            parts[2],
        );
        frame.render_widget(Paragraph::new("↑↓ select  Enter open  ←→ fold  / search  R route once\nA add path  E edit  Delete remove  F5 reload  F1 help  Esc close").style(Style::default().fg(MUTED)),parts[3]);
        if let Some(modal) = &self.modal {
            match modal {
                Modal::Search(input) => {
                    let area = popup(frame, "Search servers and saved paths");
                    frame.render_widget(
                        Paragraph::new(format!(
                            "{}\n\nEnter searches · Esc cancels · Ctrl+U clears",
                            input.text
                        ))
                        .wrap(Wrap { trim: false }),
                        area,
                    );
                }
                Modal::Edit {
                    fields, selected, ..
                } => {
                    let area = popup(frame, "Saved remote path");
                    frame.render_widget(Paragraph::new(format!("{} Name\n{}\n\n{} Remote path\n{}\n\nTab changes field · Enter saves · Esc cancels\nPaths are sent literally to the server.\n{}",if *selected==0 {">"}else{" "},fields[0].text,if *selected==1 {">"}else{" "},fields[1].text,self.notice)).wrap(Wrap{trim:false}),area);
                }
                Modal::Delete { name, .. } => {
                    let area = popup(frame, "Remove saved path?");
                    frame.render_widget(Paragraph::new(format!("{name}\n\nDelete confirms · Esc cancels\nOnly this saved shortcut is removed.\n{}",self.notice)).wrap(Wrap{trim:false}),area);
                }
                Modal::Saved => {
                    let area = popup(frame, "Saved path");
                    frame.render_widget(
                        Paragraph::new("Saved. Press Esc to return to the server list."),
                        area,
                    );
                }
                Modal::Routes { target, cursor } => {
                    let area = popup(frame, "Use route once");
                    if let Ok(machine) = self.machine(&target.machine) {
                        let parts = Layout::vertical([
                            Constraint::Length(2),
                            Constraint::Min(1),
                            Constraint::Length(3),
                        ])
                        .split(area);
                        frame.render_widget(Paragraph::new(format!("{} · Enter connects once; Esc cancels\nDevice default stays unchanged.",machine.name)),parts[0]);
                        let routes: Vec<_> = machine
                            .routes
                            .iter()
                            .map(|r| {
                                ListItem::new(Line::raw(format!(
                                    "{} · {}:{}",
                                    r.name, r.host, r.port
                                )))
                            })
                            .collect();
                        let mut state = ListState::default().with_selected(Some(*cursor));
                        frame.render_stateful_widget(
                            List::new(routes)
                                .highlight_style(
                                    Style::default()
                                        .bg(Color::Rgb(30, 61, 71))
                                        .add_modifier(Modifier::BOLD),
                                )
                                .highlight_symbol("› "),
                            parts[1],
                            &mut state,
                        );
                        frame.render_widget(
                            Paragraph::new(self.notice.clone()).wrap(Wrap { trim: false }),
                            parts[2],
                        );
                    }
                }
                Modal::Help => {
                    let area = popup(frame, "Files chooser keys");
                    frame.render_widget(Paragraph::new("Enter on a group folds it. Enter on a server opens its home.\nRight on a server reveals saved remote paths.\nEnter on a path opens it immediately after route validation.\nR chooses a route once; no automatic fallback is attempted.\nA/E add or edit named paths. Delete asks before removing one.\n/ searches names, routes and saved paths; Esc clears search.\nF5 reloads changes from other tabs.\nAdd/import servers in the normal SSH Sessions picker.\n\nEsc closes this help.").wrap(Wrap{trim:false}),area);
                }
            }
        }
    }
}
fn pick(picker: &mut Picker) -> Result<Action> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        terminal.draw(|frame| picker.draw(frame))?;
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            match picker.key(key) {
                Ok(Action::Continue) => {}
                Ok(action) => return Ok(action),
                Err(error) => picker.notice = format!("{error:#}"),
            }
        }
    }
}
pub fn run(catalog: &Catalog) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Open Files in a terminal, or specify a machine with --json."
    );
    ctrlc::set_handler(|| {}).context("Could not install terminal interrupt handling")?;
    let mut picker = Picker::new(catalog.clone())?;
    loop {
        connection::set_title("SSH Files · Choose a server");
        match pick(&mut picker)? {
            Action::Quit => return Ok(()),
            Action::Continue => continue,
            Action::Launch(launch) => {
                connection::clear_session_screen()?;
                connection::set_title(&launch.title);
                let result = connection::run_session(&launch.argv);
                if let Ok(code) = &result
                    && *code != 0
                {
                    println!(
                        "SSH Files ended (exit {code}). Read its message above, then press Enter to return."
                    );
                    let mut line = String::new();
                    io::stdin().read_line(&mut line)?;
                }
                let reload_error = picker.reload().err();
                picker.notice = match result {
                    Ok(0) => "Files closed. Choose another server or path.".into(),
                    Ok(code) => format!(
                        "Files ended (exit {code}). R selects another route explicitly; no fallback was attempted."
                    ),
                    Err(error) => format!("Files could not open: {error:#}"),
                };
                if let Some(error) = reload_error {
                    picker.notice.push_str(&format!(
                        " Reload failed: {error:#}. Press F5 before opening again."
                    ));
                }
                connection::clear_session_screen()?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Route;
    fn fixture() -> (tempfile::TempDir, Catalog) {
        let temp = tempfile::tempdir().unwrap();
        let catalog = Catalog::new(
            &temp.path().join("catalog.json"),
            &temp.path().join("device"),
        )
        .unwrap();
        let machine = Machine {
            id: "lab".into(),
            name: "Build box".into(),
            user: "dev".into(),
            group: "Work/Lab".into(),
            tags: vec![],
            routes: vec![
                Route {
                    id: "lan".into(),
                    name: "LAN".into(),
                    host: "192.0.2.1".into(),
                    port: 22,
                    ssh_alias: None,
                },
                Route {
                    id: "vpn".into(),
                    name: "VPN".into(),
                    host: "192.0.2.2".into(),
                    port: 22,
                    ssh_alias: None,
                },
            ],
        };
        catalog
            .save(&[machine], &catalog.load().unwrap().revision)
            .unwrap();
        (temp, catalog)
    }
    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }
    #[test]
    fn metadata_tree_and_route_dialog_never_launch_or_choose_a_default() {
        let (_temp, catalog) = fixture();
        let before = std::fs::read(&catalog.path).unwrap();
        let mut picker = Picker::new(catalog.clone()).unwrap();
        assert!(matches!(picker.rows[0].key, RowKey::Group(_)));
        assert!(matches!(
            picker.key(key(KeyCode::Down)).unwrap(),
            Action::Continue
        ));
        assert!(matches!(
            picker.key(key(KeyCode::Enter)).unwrap(),
            Action::Continue
        ));
        assert!(matches!(picker.modal, Some(Modal::Routes { .. })));
        assert!(!catalog.device_path.exists());
        picker.key(key(KeyCode::Esc)).unwrap();
        picker.key(key(KeyCode::Right)).unwrap();
        assert!(picker.expanded.contains("lab"));
        assert_eq!(before, std::fs::read(&catalog.path).unwrap());
        assert!(!file_presets::sidecar_path(&catalog).exists());
    }
    #[test]
    fn save_preserves_literal_path_and_trailing_newlines_cannot_open_a_session() {
        let (_temp, catalog) = fixture();
        let mut picker = Picker::new(catalog.clone()).unwrap();
        picker.select_key(&RowKey::Machine("lab".into()));
        picker.edit(false).unwrap();
        if let Some(Modal::Edit { fields, .. }) = &mut picker.modal {
            fields[0] = Input::new("Project");
            fields[1] = Input::new("/srv/code ; $HOME/開發");
        } else {
            panic!("missing editor")
        }
        for _ in 0..3 {
            assert!(matches!(
                picker.key(key(KeyCode::Enter)).unwrap(),
                Action::Continue
            ));
        }
        assert!(matches!(picker.modal, Some(Modal::Saved)));
        let (presets, _) = FilePresets::load(&catalog).unwrap();
        assert_eq!(presets.for_machine("lab")[0].path, "/srv/code ; $HOME/開發");
        assert!(!catalog.device_path.exists());
        picker.key(key(KeyCode::Esc)).unwrap();
        assert!(matches!(picker.selected_key(), Some(RowKey::Path(_, _))));
        picker.query = "no match".into();
        picker.rebuild(true);
        assert!(picker.selected.is_none());
        assert!(matches!(
            picker.key(key(KeyCode::Enter)).unwrap(),
            Action::Continue
        ));
    }
    #[test]
    fn malformed_presets_keep_home_usable_and_stale_edits_retain_fields() {
        let (_temp, catalog) = fixture();
        std::fs::write(file_presets::sidecar_path(&catalog), "broken").unwrap();
        let mut picker = Picker::new(catalog.clone()).unwrap();
        assert!(picker.presets.is_none());
        assert!(picker.preset_error.contains("Server home still works"));
        picker.select_key(&RowKey::Machine("lab".into()));
        assert!(matches!(
            picker.key(key(KeyCode::Enter)).unwrap(),
            Action::Continue
        ));
        assert!(matches!(picker.modal, Some(Modal::Routes { .. })));
        picker.key(key(KeyCode::Esc)).unwrap();
        assert!(picker.edit(false).is_err());
        std::fs::remove_file(file_presets::sidecar_path(&catalog)).unwrap();
        picker.reload().unwrap();
        picker.edit(false).unwrap();
        if let Some(Modal::Edit { fields, .. }) = &mut picker.modal {
            fields[0] = Input::new("Unsaved");
        }
        let (mut presets, revision) = FilePresets::load(&catalog).unwrap();
        presets
            .upsert(
                &catalog,
                "lab",
                Preset {
                    id: "other".into(),
                    name: "Other view".into(),
                    path: "/other".into(),
                },
                &catalog.load().unwrap().revision,
                &revision,
            )
            .unwrap();
        assert!(picker.key(key(KeyCode::Enter)).is_err());
        assert!(matches!(&picker.modal,Some(Modal::Edit{fields,..}) if fields[0].text=="Unsaved"));
    }
    #[test]
    fn control_characters_and_oversized_text_are_not_renderable_editor_input() {
        let mut input = Input::default();
        for character in ['a', '\u{1b}', '\u{85}', '開', 'z'] {
            input_key(&mut input, key(KeyCode::Char(character)), 4);
        }
        assert_eq!(input.text, "a開");
        input_key(
            &mut input,
            KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL),
            4,
        );
        assert!(input.text.is_empty());
    }
}
