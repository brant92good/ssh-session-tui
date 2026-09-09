use super::*;
use crate::sync::GitSync;

impl Picker {
    pub(super) fn handle(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match self.screen.clone() {
            Screen::Main => self.main_key(key)?,
            Screen::Search => match key.code {
                KeyCode::Enter => self.screen = Screen::Main,
                KeyCode::Esc => {
                    self.query = Input::default();
                    self.selected = 0;
                    self.screen = Screen::Main;
                }
                _ => {
                    self.query.key(key);
                    self.selected = 0;
                }
            },
            Screen::Form(mut form) => match key.code {
                KeyCode::Esc => {
                    self.screen = *form.return_to;
                    self.notice.clear();
                }
                KeyCode::Char('s') if ctrl => self.save_form(&form)?,
                KeyCode::Enter if matches!(form.edit, Edit::ImportPath) => self.save_form(&form)?,
                KeyCode::Tab | KeyCode::Down | KeyCode::Enter => {
                    form.selected = (form.selected + 1) % form.fields.len();
                    self.screen = Screen::Form(form);
                }
                KeyCode::BackTab | KeyCode::Up => {
                    form.selected = (form.selected + form.fields.len() - 1) % form.fields.len();
                    self.screen = Screen::Form(form);
                }
                _ => {
                    let selected = form.selected;
                    form.fields[selected].input.key(key);
                    self.screen = Screen::Form(form);
                }
            },
            Screen::Favorite {
                target,
                revision,
                mut selected,
            } => match key.code {
                KeyCode::Esc => self.screen = Screen::Main,
                KeyCode::Char('1'..='9') => {
                    if let KeyCode::Char(c) = key.code {
                        selected = c as usize - '1' as usize;
                        self.screen = Screen::Favorite {
                            target,
                            revision,
                            selected,
                        };
                    }
                }
                KeyCode::Up | KeyCode::Down => {
                    selected = if key.code == KeyCode::Up {
                        selected.saturating_sub(1)
                    } else {
                        (selected + 1).min(8)
                    };
                    self.screen = Screen::Favorite {
                        target,
                        revision,
                        selected,
                    };
                }
                KeyCode::Enter | KeyCode::Char('d' | 'D') | KeyCode::Delete => {
                    let slot = (selected + 1).to_string();
                    let save = key.code == KeyCode::Enter;
                    Favorites::assign(
                        &self.catalog,
                        &slot,
                        if save { Some(&target) } else { None },
                        Some(&revision),
                    )?;
                    self.reload()?;
                    self.screen = Screen::Main;
                    self.notice = format!(
                        "Favorite {slot} {}.",
                        if save { "saved" } else { "cleared" }
                    );
                }
                _ => {}
            },
            Screen::Routes {
                machine,
                mut selected,
                fallback,
                connect_after,
            } => {
                let current = self.machine(&machine)?.clone();
                selected = selected.min(current.routes.len().saturating_sub(1));
                match key.code {
                    KeyCode::Esc => self.screen = Screen::Main,
                    KeyCode::Up | KeyCode::Down => {
                        selected = if key.code == KeyCode::Up {
                            selected.saturating_sub(1)
                        } else {
                            (selected + 1).min(current.routes.len().saturating_sub(1))
                        };
                        self.screen = Screen::Routes {
                            machine,
                            selected,
                            fallback,
                            connect_after,
                        };
                    }
                    KeyCode::Enter => {
                        self.ensure_current()?;
                        let route = current
                            .routes
                            .get(selected)
                            .context("Choose a route.")?
                            .clone();
                        if !fallback {
                            self.catalog.choose(&current, &route.id)?;
                            self.reload()?;
                        }
                        if fallback || connect_after {
                            self.choice = Some(Choice::Connect(Box::new(current), route));
                        } else {
                            self.screen = Screen::Main;
                            self.notice = format!("Using {} on this device.", route.name);
                        }
                    }
                    KeyCode::Char('a' | 'A') => self.route_form(&machine, None)?,
                    KeyCode::Char('e' | 'E') => self.route_form(&machine, Some(selected))?,
                    KeyCode::Char('d' | 'D') => {
                        ensure!(
                            current.routes.len() > 1,
                            "Keep at least one route, or delete the machine."
                        );
                        let route = &current.routes[selected];
                        self.screen = Screen::Confirm {
                            message: format!("Delete route {}?", route.name),
                            action: Delete::Route(machine, route.id.clone()),
                            revision: self.snapshot.revision.clone(),
                            return_to: Box::new(self.screen.clone()),
                        };
                    }
                    _ => {}
                }
            }
            Screen::Groups { mut selected } => {
                let paths: Vec<_> = organization::groups(&self.snapshot.machines)
                    .into_keys()
                    .collect();
                match key.code {
                    KeyCode::Esc => self.screen = Screen::Main,
                    KeyCode::Up | KeyCode::Down => {
                        selected = if key.code == KeyCode::Up {
                            selected.saturating_sub(1)
                        } else {
                            (selected + 1).min(paths.len() + 1)
                        };
                        self.screen = Screen::Groups { selected };
                    }
                    KeyCode::Enter => {
                        self.group = if selected == 0 {
                            None
                        } else if selected == 1 {
                            Some(String::new())
                        } else {
                            Some(paths.get(selected - 2).context("Reload groups.")?.clone())
                        };
                        self.marked.clear();
                        self.selected = 0;
                        self.query = Input::default();
                        self.screen = Screen::Main;
                    }
                    KeyCode::Char('e' | 'E') => {
                        let old = paths
                            .get(
                                selected
                                    .checked_sub(2)
                                    .context("Select a named group to rename.")?,
                            )
                            .context("Select a group.")?
                            .clone();
                        self.form(
                            "Rename group and subgroups",
                            Edit::Rename(old.clone()),
                            vec![("New group path", old)],
                        );
                    }
                    _ => {}
                }
            }
            Screen::Import {
                scan,
                mut selected,
                mut marked,
                group,
                revision,
            } => {
                match key.code {
                    KeyCode::Esc => {
                        self.screen = Screen::Main;
                        return Ok(());
                    }
                    KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Down => {
                        selected = (selected + 1).min(scan.entries.len().saturating_sub(1))
                    }
                    KeyCode::Char(' ') => {
                        if let Some(entry) = scan.entries.get(selected) {
                            let status = ssh_import::status(&self.snapshot.machines, entry);
                            ensure!(
                                matches!(status.as_str(), ssh_import::READY | ssh_import::IMPORTED),
                                "{status}"
                            );
                            if !marked.remove(&entry.alias) {
                                marked.insert(entry.alias.clone());
                            }
                        }
                    }
                    KeyCode::Char('a' | 'A') => {
                        let ready: BTreeSet<_> = scan
                            .entries
                            .iter()
                            .filter(|e| {
                                matches!(
                                    ssh_import::status(&self.snapshot.machines, e).as_str(),
                                    ssh_import::READY | ssh_import::IMPORTED
                                )
                            })
                            .map(|e| e.alias.clone())
                            .collect();
                        if marked == ready {
                            marked.clear();
                        } else {
                            marked = ready;
                        }
                    }
                    KeyCode::Char('c' | 'C') | KeyCode::Tab => {
                        self.form(
                            "Choose SSH config",
                            Edit::ImportPath,
                            vec![(
                                "SSH config file",
                                scan.config.to_string_lossy().into_owned(),
                            )],
                        );
                        return Ok(());
                    }
                    KeyCode::Char('g' | 'G') => {
                        self.form(
                            "Group for imported machines",
                            Edit::ImportGroup,
                            vec![("Group path (optional)", group)],
                        );
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        if marked.is_empty()
                            && let Some(entry) = scan.entries.get(selected)
                        {
                            marked.insert(entry.alias.clone());
                        }
                        let result = ssh_import::import(
                            &self.catalog,
                            &scan,
                            &marked.into_iter().collect::<Vec<_>>(),
                            &revision,
                            &group,
                        )?;
                        self.reload()?;
                        self.notice = format!(
                            "Imported {} machines, {} routes; bound {} existing aliases.",
                            result.added_machines, result.added_routes, result.bound_existing
                        );
                        self.screen = Screen::Main;
                        return Ok(());
                    }
                    _ => {}
                }
                self.screen = Screen::Import {
                    scan,
                    selected,
                    marked,
                    group,
                    revision,
                };
            }
            Screen::Confirm {
                message: _,
                action,
                revision,
                return_to,
            } => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    self.delete(&action, &revision)?;
                    self.screen = *return_to;
                }
                KeyCode::Esc | KeyCode::Char('n' | 'N') => self.screen = *return_to,
                _ => {}
            },
            Screen::Help => {
                if matches!(key.code, KeyCode::Esc | KeyCode::F(1) | KeyCode::Enter) {
                    self.screen = Screen::Main;
                }
            }
            Screen::Sync => match key.code {
                KeyCode::Esc => self.screen = Screen::Main,
                KeyCode::Char('p' | 'P' | 'u' | 'U') => {
                    let action = if matches!(key.code, KeyCode::Char('p' | 'P')) {
                        "pull"
                    } else {
                        "publish"
                    };
                    let catalog = self.catalog.clone();
                    let (sender, receiver) = mpsc::channel();
                    std::thread::spawn(move || {
                        let result = GitSync::new(&catalog)
                            .and_then(|sync| sync.run(action))
                            .map_err(|e| format!("{e:#}"));
                        let _ = sender.send(result);
                    });
                    self.pending = Some(receiver);
                    self.screen = Screen::Busy;
                    self.notice = format!("Git {action} in progress…");
                }
                _ => {}
            },
            Screen::Busy => {}
        }
        Ok(())
    }
    fn main_key(&mut self, key: KeyEvent) -> Result<()> {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if matches!(
            key.code,
            KeyCode::Up
                | KeyCode::Down
                | KeyCode::Home
                | KeyCode::End
                | KeyCode::PageUp
                | KeyCode::PageDown
                | KeyCode::Esc
                | KeyCode::Char('/')
        ) {
            self.selection_valid = true;
        }
        match key.code {
            KeyCode::Up => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down => {
                self.selected = (self.selected + 1).min(self.rows().len().saturating_sub(1))
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.rows().len().saturating_sub(1),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.rows().len().saturating_sub(1))
            }
            KeyCode::Enter => self.connect(&self.current())?,
            KeyCode::Char('l') if ctrl => self.choice = Some(Choice::Local),
            KeyCode::Char('c') if ctrl => self.choice = Some(Choice::Quit),
            KeyCode::Char('a') if ctrl => {
                let ids: BTreeSet<_> = self.rows().into_iter().filter(|id| id != LOCAL).collect();
                if self.marked == ids {
                    self.marked.clear();
                } else {
                    self.marked = ids;
                }
            }
            KeyCode::Char('q' | 'Q') => self.choice = Some(Choice::Quit),
            KeyCode::Char('/') => {
                self.marked.clear();
                self.screen = Screen::Search;
            }
            KeyCode::Esc => {
                self.query = Input::default();
                self.group = None;
                self.marked.clear();
                self.selected = 0;
            }
            KeyCode::Char('1'..='9') if !ctrl => {
                self.selection_valid = false;
                if let KeyCode::Char(c) = key.code {
                    let target = self
                        .favorites
                        .slots
                        .get(&c.to_string())
                        .cloned()
                        .context("That number has no favorite. Select a row and press F.")?;
                    ensure!(
                        target == LOCAL || self.snapshot.machines.iter().any(|m| m.id == target),
                        "That favorite's machine is missing. Press F to replace it."
                    );
                    self.query = Input::default();
                    self.group = None;
                    self.marked.clear();
                    self.selected = self.rows().iter().position(|id| id == &target).unwrap_or(0);
                    self.selection_valid = true;
                    self.notice = "Press Enter to connect.".into();
                }
            }
            KeyCode::Char('f' | 'F') => {
                self.screen = Screen::Favorite {
                    target: self.current(),
                    revision: Favorites::load(&self.catalog)?.1,
                    selected: 0,
                }
            }
            KeyCode::Char(' ') => {
                let id = self.current();
                if id != LOCAL && !self.marked.remove(&id) {
                    self.marked.insert(id);
                }
            }
            KeyCode::Char('g' | 'G') => self.screen = Screen::Groups { selected: 0 },
            KeyCode::Char('m' | 'M') => self.form(
                "Move machines",
                Edit::Move(self.targets()?),
                vec![(
                    "Group path (empty for Ungrouped)",
                    self.group.clone().unwrap_or_default(),
                )],
            ),
            KeyCode::Char('t' | 'T') => self.form(
                "Edit tags",
                Edit::Tags(self.targets()?),
                vec![
                    ("Add tags, separated by commas", String::new()),
                    ("Remove tags, separated by commas", String::new()),
                ],
            ),
            KeyCode::Char('a' | 'A') => self.form(
                "Add a machine",
                Edit::AddMachine,
                vec![
                    ("Machine name", String::new()),
                    ("SSH username", String::new()),
                    ("Route name", "LAN".into()),
                    ("IP address, hostname or SSH alias", String::new()),
                    ("SSH port", "22".into()),
                    (
                        "Group path (optional)",
                        self.group.clone().unwrap_or_default(),
                    ),
                    ("Tags, separated by commas", String::new()),
                ],
            ),
            KeyCode::Char('e' | 'E') => {
                let machine = self.machine(&self.current())?.clone();
                self.form(
                    "Edit machine",
                    Edit::Machine(machine.id),
                    vec![
                        ("Machine name", machine.name),
                        ("SSH username", machine.user),
                        ("Group path", machine.group),
                        ("Tags, separated by commas", machine.tags.join(", ")),
                    ],
                );
            }
            KeyCode::Char('d' | 'D') => {
                let ids = self.targets()?;
                self.screen = Screen::Confirm {
                    message: format!("Delete {} selected machine(s)?", ids.len()),
                    action: Delete::Machines(ids),
                    revision: self.snapshot.revision.clone(),
                    return_to: Box::new(Screen::Main),
                };
            }
            KeyCode::Char('r' | 'R') => {
                let machine = self.machine(&self.current())?.id.clone();
                self.screen = Screen::Routes {
                    machine,
                    selected: 0,
                    fallback: false,
                    connect_after: false,
                };
            }
            KeyCode::Char('i' | 'I') => match ssh_import::scan(None) {
                Ok(scan) => {
                    self.screen = Screen::Import {
                        scan,
                        selected: 0,
                        marked: BTreeSet::new(),
                        group: String::new(),
                        revision: self.snapshot.revision.clone(),
                    }
                }
                Err(error) => {
                    self.form(
                        "Import SSH hosts",
                        Edit::ImportPath,
                        vec![(
                            "SSH config file",
                            ssh_import::default_config().to_string_lossy().into_owned(),
                        )],
                    );
                    self.notice = error.to_string();
                }
            },
            KeyCode::Char('s' | 'S') => self.screen = Screen::Sync,
            KeyCode::F(1) => self.screen = Screen::Help,
            KeyCode::F(5) => {
                self.reload()?;
                self.notice = "Reloaded.".into();
            }
            _ => {}
        }
        Ok(())
    }
}
