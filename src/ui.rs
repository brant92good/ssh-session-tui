//! Picker state is separate from rendering and terminal/process ownership.
mod draw;
mod events;
#[cfg(test)]
mod tests;
use crate::{
    catalog::{self, Catalog, Machine, Route, Snapshot},
    connection,
    favorites::{Favorites, LOCAL},
    organization,
    ssh_import::{self, Scan},
};
use anyhow::{Context, Result, ensure};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, IsTerminal},
    sync::mpsc,
    time::Duration,
};

#[derive(Debug, Clone, Default)]
struct Input {
    text: String,
    cursor: usize,
}
impl Input {
    fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }
    fn key(&mut self, key: KeyEvent) {
        let previous = || {
            self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0)
        };
        let next = || {
            self.text[self.cursor..]
                .chars()
                .next()
                .map(|c| self.cursor + c.len_utf8())
                .unwrap_or(self.cursor)
        };
        match key.code {
            KeyCode::Left => self.cursor = previous(),
            KeyCode::Right => self.cursor = next(),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.text.len(),
            KeyCode::Backspace => {
                let start = previous();
                self.text.drain(start..self.cursor);
                self.cursor = start;
            }
            KeyCode::Delete => {
                let end = next();
                self.text.drain(self.cursor..end);
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.text.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char(c)
                if !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                self.text.insert(self.cursor, c);
                self.cursor += c.len_utf8();
            }
            _ => {}
        }
    }
}
#[derive(Debug, Clone)]
struct Field {
    caption: String,
    input: Input,
}
#[derive(Debug, Clone)]
enum Edit {
    AddMachine,
    Machine(String),
    Route {
        machine: String,
        route: Option<String>,
    },
    Move(Vec<String>),
    Tags(Vec<String>),
    Rename(String),
    ImportPath,
    ImportGroup,
}
#[derive(Debug, Clone)]
struct Form {
    title: String,
    fields: Vec<Field>,
    selected: usize,
    edit: Edit,
    revision: String,
    return_to: Box<Screen>,
}
#[derive(Debug, Clone)]
enum Delete {
    Machines(Vec<String>),
    Route(String, String),
}
#[derive(Debug, Clone)]
enum Screen {
    Main,
    Search,
    Form(Form),
    Routes {
        machine: String,
        selected: usize,
        fallback: bool,
        connect_after: bool,
    },
    Groups {
        selected: usize,
    },
    Import {
        scan: Scan,
        selected: usize,
        marked: BTreeSet<String>,
        group: String,
        revision: String,
    },
    Favorite {
        target: String,
        revision: String,
    },
    Confirm {
        message: String,
        action: Delete,
        revision: String,
        return_to: Box<Screen>,
    },
    Help,
    Sync,
    Busy,
}
#[derive(Debug, Clone)]
pub enum Choice {
    Local,
    Connect(Box<Machine>, Route),
    Quit,
}

pub struct Picker {
    catalog: Catalog,
    snapshot: Snapshot,
    preferences: BTreeMap<String, String>,
    favorites: Favorites,
    local_shell: String,
    screen: Screen,
    selected: usize,
    query: Input,
    group: Option<String>,
    marked: BTreeSet<String>,
    pub notice: String,
    choice: Option<Choice>,
    pending: Option<mpsc::Receiver<std::result::Result<String, String>>>,
}
impl Picker {
    pub fn new(catalog: Catalog) -> Result<Self> {
        let snapshot = catalog.load()?;
        let preferences = catalog.preferences()?;
        let favorites = Favorites::load(&catalog)?.0;
        Ok(Self {
            catalog,
            snapshot,
            preferences,
            favorites,
            local_shell: connection::local_shell_name(),
            screen: Screen::Main,
            selected: 0,
            query: Input::default(),
            group: None,
            marked: BTreeSet::new(),
            notice: String::new(),
            choice: None,
            pending: None,
        })
    }
    fn reload(&mut self) -> Result<()> {
        let snapshot = self.catalog.load()?;
        let preferences = self.catalog.preferences()?;
        let favorites = Favorites::load(&self.catalog)?.0;
        self.snapshot = snapshot;
        self.preferences = preferences;
        self.favorites = favorites;
        self.marked
            .retain(|id| self.snapshot.machines.iter().any(|m| &m.id == id));
        self.selected = self.selected.min(self.rows().len().saturating_sub(1));
        Ok(())
    }
    fn rows(&self) -> Vec<String> {
        let mut rows = vec![LOCAL.into()];
        rows.extend(
            organization::filtered(
                &self.snapshot.machines,
                &self.query.text,
                self.group.as_deref(),
                None,
            )
            .into_iter()
            .map(|m| m.id.clone()),
        );
        rows
    }
    fn machine(&self, id: &str) -> Result<&Machine> {
        self.snapshot
            .machines
            .iter()
            .find(|m| m.id == id)
            .context("Machine was removed. Press F5 to reload.")
    }
    fn current(&self) -> String {
        self.rows()
            .get(self.selected)
            .cloned()
            .unwrap_or_else(|| LOCAL.into())
    }
    fn name(&self, id: &str) -> String {
        if id == LOCAL {
            "Local terminal".into()
        } else {
            self.machine(id)
                .map(|m| m.name.clone())
                .unwrap_or_else(|_| "Missing machine".into())
        }
    }
    fn targets(&self) -> Result<Vec<String>> {
        if self.marked.is_empty() {
            let id = self.current();
            ensure!(id != LOCAL, "Select a remote machine first.");
            Ok(vec![id])
        } else {
            Ok(self.marked.iter().cloned().collect())
        }
    }
    fn form(&mut self, title: &str, edit: Edit, fields: Vec<(&str, String)>) {
        self.notice.clear();
        self.screen = Screen::Form(Form {
            title: title.into(),
            fields: fields
                .into_iter()
                .map(|(caption, value)| Field {
                    caption: caption.into(),
                    input: Input::new(value),
                })
                .collect(),
            selected: 0,
            edit,
            revision: self.snapshot.revision.clone(),
            return_to: Box::new(self.screen.clone()),
        });
    }
    fn route_form(&mut self, machine: &str, selected: Option<usize>) -> Result<()> {
        let route = selected.and_then(|i| self.machine(machine).ok()?.routes.get(i));
        let fields = vec![
            (
                "Route name (LAN, Tailscale…)",
                route.map(|r| r.name.clone()).unwrap_or_default(),
            ),
            (
                "IP address, hostname or SSH alias",
                route.map(|r| r.host.clone()).unwrap_or_default(),
            ),
            (
                "SSH port",
                route
                    .map(|r| r.port.to_string())
                    .unwrap_or_else(|| "22".into()),
            ),
        ];
        let edit = Edit::Route {
            machine: machine.into(),
            route: route.map(|r| r.id.clone()),
        };
        self.form(
            if selected.is_some() {
                "Edit route"
            } else {
                "Add route"
            },
            edit,
            fields,
        );
        Ok(())
    }
    fn connect(&mut self, id: &str) -> Result<()> {
        if id == LOCAL {
            self.choice = Some(Choice::Local);
            return Ok(());
        }
        let machine = self.machine(id)?.clone();
        if let Some(route) = self.catalog.preferred(&machine, &self.preferences) {
            self.choice = Some(Choice::Connect(Box::new(machine.clone()), route.clone()));
        } else {
            self.screen = Screen::Routes {
                machine: machine.id,
                selected: 0,
                fallback: false,
                connect_after: true,
            };
        }
        Ok(())
    }
    fn save_form(&mut self, form: &Form) -> Result<()> {
        let values: Vec<_> = form.fields.iter().map(|f| f.input.text.as_str()).collect();
        let mut machines = self.snapshot.machines.clone();
        match &form.edit {
            Edit::AddMachine => {
                machines.push(Machine {
                    id: catalog::id(),
                    name: catalog::label(values[0])?,
                    user: catalog::login(values[1])?,
                    group: catalog::group_path(values[5])?,
                    tags: catalog::parse_tags(values[6])?,
                    routes: vec![Route {
                        id: catalog::id(),
                        name: catalog::label(values[2])?,
                        host: catalog::address(values[3])?,
                        port: catalog::port(values[4])?,
                        ssh_alias: None,
                    }],
                });
                self.catalog.save(&machines, &form.revision)?;
            }
            Edit::Machine(id) => {
                let machine = machines
                    .iter_mut()
                    .find(|m| &m.id == id)
                    .context("Machine was removed.")?;
                machine.name = catalog::label(values[0])?;
                machine.user = catalog::login(values[1])?;
                machine.group = catalog::group_path(values[2])?;
                machine.tags = catalog::parse_tags(values[3])?;
                self.catalog.save(&machines, &form.revision)?;
            }
            Edit::Route { machine, route } => {
                let machine = machines
                    .iter_mut()
                    .find(|m| &m.id == machine)
                    .context("Machine was removed.")?;
                let old = route
                    .as_ref()
                    .and_then(|id| machine.routes.iter().find(|r| &r.id == id));
                ensure!(route.is_none() || old.is_some(), "Route was removed.");
                let replacement = Route {
                    id: route.clone().unwrap_or_else(catalog::id),
                    name: catalog::label(values[0])?,
                    host: catalog::address(values[1])?,
                    port: catalog::port(values[2])?,
                    ssh_alias: old.and_then(|r| r.ssh_alias.clone()),
                };
                if let Some(route) = machine.routes.iter_mut().find(|r| r.id == replacement.id) {
                    *route = replacement;
                } else {
                    machine.routes.push(replacement);
                }
                self.catalog.save(&machines, &form.revision)?;
            }
            Edit::Move(ids) => {
                organization::edit_many(
                    &self.catalog,
                    ids,
                    &form.revision,
                    Some(values[0]),
                    &[],
                    &[],
                )?;
                self.marked.clear();
            }
            Edit::Tags(ids) => {
                organization::edit_many(
                    &self.catalog,
                    ids,
                    &form.revision,
                    None,
                    &catalog::parse_tags(values[0])?,
                    &catalog::parse_tags(values[1])?,
                )?;
                self.marked.clear();
            }
            Edit::Rename(old) => {
                organization::rename_group(&self.catalog, old, values[0], &form.revision)?;
            }
            Edit::ImportPath => {
                let scan = ssh_import::scan(Some(&ssh_import::expand_home(values[0])))?;
                self.screen = Screen::Import {
                    scan,
                    selected: 0,
                    marked: BTreeSet::new(),
                    group: String::new(),
                    revision: self.catalog.load()?.revision,
                };
                self.notice.clear();
                return Ok(());
            }
            Edit::ImportGroup => {
                let mut screen = *form.return_to.clone();
                if let Screen::Import { group, .. } = &mut screen {
                    *group = catalog::group_path(values[0])?;
                }
                self.screen = screen;
                return Ok(());
            }
        }
        self.reload()?;
        self.screen = *form.return_to.clone();
        self.notice = "Saved.".into();
        Ok(())
    }
    fn delete(&mut self, action: &Delete, revision: &str) -> Result<()> {
        let mut machines = self.snapshot.machines.clone();
        match action {
            Delete::Machines(ids) => machines.retain(|m| !ids.contains(&m.id)),
            Delete::Route(machine, route) => {
                let machine = machines
                    .iter_mut()
                    .find(|m| &m.id == machine)
                    .context("Machine was removed.")?;
                ensure!(
                    machine.routes.len() > 1,
                    "Keep at least one route, or delete the machine."
                );
                machine.routes.retain(|r| &r.id != route);
            }
        }
        self.catalog.save(&machines, revision)?;
        self.reload()?;
        self.notice = "Deleted.".into();
        Ok(())
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Release
            && let Err(error) = self.handle(key)
        {
            self.notice = format!("{error:#}");
        }
    }
    fn poll(&mut self) {
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.notice = result.unwrap_or_else(|e| e);
                    self.pending = None;
                    self.screen = Screen::Main;
                    if let Err(error) = self.reload() {
                        self.notice = error.to_string();
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending = None;
                    self.screen = Screen::Main;
                    self.notice = "Sync worker stopped unexpectedly.".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
}
struct TerminalGuard;
impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show);
    }
}
fn pick(picker: &mut Picker) -> Result<Choice> {
    let _guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    loop {
        picker.poll();
        terminal.draw(|frame| picker.draw(frame))?;
        if let Some(choice) = picker.choice.take() {
            return Ok(choice);
        }
        if event::poll(Duration::from_millis(150))?
            && let Event::Key(key) = event::read()?
        {
            picker.key(key);
        }
    }
}
pub fn picker_loop(catalog: &Catalog) -> Result<()> {
    ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "Open ssh-sessions in a terminal, or use list --json for scripts."
    );
    // Caught signals reset to defaults in the executed Unix child. The parent survives
    // Ctrl+C while the local shell/OpenSSH handles its own foreground signals.
    ctrlc::set_handler(|| {}).context("Could not install terminal interrupt handling")?;
    let mut notice = String::new();
    let mut failed: Option<String> = None;
    loop {
        connection::set_title("SSH Sessions");
        let mut picker = Picker::new(catalog.clone())?;
        picker.notice = notice;
        if let Some(machine) = failed.take()
            && picker.machine(&machine).is_ok()
        {
            picker.screen = Screen::Routes {
                machine,
                selected: 0,
                fallback: true,
                connect_after: true,
            };
        }
        let choice = pick(&mut picker)?; // Guard restores normal terminal mode before spawn.
        let (args, title, remote) = match choice {
            Choice::Quit => return Ok(()),
            Choice::Local => (
                connection::local_command(),
                format!("Local {}", connection::local_shell_name()),
                None,
            ),
            Choice::Connect(machine, route) => {
                let args =
                    ssh_import::connection_config(catalog, &machine, &route).and_then(|config| {
                        connection::ssh_command(&machine, &route, config.as_deref())
                    });
                println!(
                    "Connecting to {} via {} ({}:{})",
                    machine.name, route.name, route.host, route.port
                );
                (
                    args,
                    format!("{} | {}", machine.name, route.name),
                    Some(machine.id),
                )
            }
        };
        connection::set_title(&title);
        match args.and_then(|args| connection::run_session(&args)) {
            Ok(255) if remote.is_some() => {
                failed = remote;
                println!(
                    "SSH failed. Read its message above, then press Enter to return and choose a route."
                );
                let mut line = String::new();
                io::stdin().read_line(&mut line)?;
                notice = "Connection failed. Select another route to try once.".into();
            }
            Ok(code) => {
                notice = format!(
                    "{} ended (exit {code}). Choose a machine to connect again.",
                    if remote.is_some() {
                        "SSH session"
                    } else {
                        "Local shell"
                    }
                )
            }
            Err(error) => notice = format!("{error:#}"),
        }
    }
}
