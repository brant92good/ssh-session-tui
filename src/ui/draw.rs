use super::*;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;
const ACCENT: Color = Color::Rgb(101, 214, 190);
const MUTED: Color = Color::Rgb(148, 163, 184);
const BG: Color = Color::Rgb(15, 23, 42);
const TEXT: Color = Color::Rgb(226, 232, 240);
const SELECTED: Color = Color::Rgb(30, 61, 71);

impl Picker {
    pub fn draw(&self, frame: &mut Frame<'_>) {
        let area = frame.area();
        frame.render_widget(
            Block::default().style(Style::default().bg(BG).fg(TEXT)),
            area,
        );
        let parts = Layout::vertical([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    "  SSH Sessions",
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
                ),
                Span::styled("   Your machines", Style::default().fg(MUTED)),
            ])),
            parts[0],
        );
        let favorites = (1..=9)
            .filter_map(|n| {
                self.favorites
                    .slots
                    .get(&n.to_string())
                    .map(|id| format!("{n} {}", self.name(id)))
            })
            .collect::<Vec<_>>()
            .join("   ·   ");
        frame.render_widget(
            Paragraph::new(if favorites.is_empty() {
                "  Select a row and press F to add a numbered favorite.".into()
            } else {
                format!("  {favorites}")
            })
            .style(Style::default().fg(ACCENT))
            .wrap(Wrap { trim: false }),
            parts[1],
        );
        let group = self
            .group
            .as_deref()
            .map(|g| if g.is_empty() { "Ungrouped" } else { g })
            .unwrap_or("All machines");
        frame.render_widget(
            Paragraph::new(format!(
                "  {group}   / Search: {}{}",
                self.query.text,
                if matches!(self.screen, Screen::Search) {
                    "▏"
                } else {
                    ""
                }
            ))
            .style(Style::default().fg(MUTED)),
            parts[2],
        );
        let items: Vec<_> = self
            .rows()
            .iter()
            .map(|id| {
                let slot = self
                    .favorites
                    .slots
                    .iter()
                    .find(|(_, t)| *t == id)
                    .map(|(s, _)| s.as_str())
                    .unwrap_or(" ");
                let mark = if self.marked.contains(id) { "✓" } else { " " };
                if id == LOCAL {
                    ListItem::new(format!(
                        "{mark} {slot}  Local terminal     {} on this computer",
                        self.local_shell
                    ))
                } else if let Ok(machine) = self.machine(id) {
                    let route = self
                        .catalog
                        .preferred(machine, &self.preferences)
                        .map(|r| format!("{}@{}:{}  ·  {}", machine.user, r.host, r.port, r.name))
                        .unwrap_or_else(|| "Choose a route".into());
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::raw(format!("{mark} {slot}  ")),
                            Span::styled(
                                machine.name.clone(),
                                Style::default().add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("   {}", machine.group),
                                Style::default().fg(MUTED),
                            ),
                        ]),
                        Line::styled(
                            format!(
                                "     {route}{}",
                                if machine.tags.is_empty() {
                                    String::new()
                                } else {
                                    format!("  #{}", machine.tags.join(" #"))
                                }
                            ),
                            Style::default().fg(MUTED),
                        ),
                    ])
                } else {
                    ListItem::new("Missing machine")
                }
            })
            .collect();
        let title = format!(
            " {} machines{} ",
            self.rows().len().saturating_sub(1),
            if self.marked.is_empty() {
                String::new()
            } else {
                format!(", {} selected", self.marked.len())
            }
        );
        frame.render_stateful_widget(
            List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::TOP | Borders::BOTTOM)
                        .title(title)
                        .border_style(Style::default().fg(MUTED)),
                )
                .highlight_style(Style::default().bg(SELECTED))
                .highlight_symbol("› "),
            parts[3],
            &mut ListState::default().with_selected(Some(self.selected)),
        );
        frame.render_widget(
            Paragraph::new(
                if self.snapshot.machines.is_empty() && self.notice.is_empty() {
                    "Press I to import existing SSH hosts, or A to add your first machine."
                } else {
                    &self.notice
                },
            )
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(ACCENT)),
            parts[4],
        );
        frame.render_widget(Paragraph::new("Enter connect  1–9 favorite  A add  I import  R routes  G groups  / search\nF favorite  Space select  M move  T tags  S sync  F1 help  Q quit").style(Style::default().fg(MUTED)),parts[5]);
        self.draw_modal(frame, area);
    }
    fn draw_modal(&self, frame: &mut Frame<'_>, area: Rect) {
        match &self.screen {
            Screen::Main | Screen::Search => {}
            Screen::Form(form) => {
                let rect = modal(
                    frame,
                    area,
                    &form.title,
                    72,
                    (form.fields.len() as u16 * 3 + 7).min(31),
                );
                let chunks = Layout::vertical([
                    Constraint::Min(1),
                    Constraint::Length(2),
                    Constraint::Length(2),
                ])
                .split(rect);
                let visible = (chunks[0].height / 3).max(1) as usize;
                let start = form.selected.saturating_sub(visible - 1);
                for (i, field) in form.fields.iter().enumerate().skip(start).take(visible) {
                    let y = chunks[0].y + ((i - start) * 3) as u16;
                    if y + 1 >= chunks[0].bottom() {
                        break;
                    }
                    let active = i == form.selected;
                    let label = Rect::new(chunks[0].x, y, chunks[0].width, 1);
                    let input_area = Rect::new(chunks[0].x, y + 1, chunks[0].width, 1);
                    frame.render_widget(
                        Paragraph::new(field.caption.as_str())
                            .style(Style::default().fg(if active { ACCENT } else { MUTED })),
                        label,
                    );
                    let available = input_area.width.saturating_sub(2) as usize;
                    let mut start_byte = 0;
                    while UnicodeWidthStr::width(&field.input.text[start_byte..field.input.cursor])
                        >= available.max(1)
                        && start_byte < field.input.cursor
                    {
                        start_byte += field.input.text[start_byte..]
                            .chars()
                            .next()
                            .map(char::len_utf8)
                            .unwrap_or(1);
                    }
                    frame.render_widget(
                        Paragraph::new(format!(" {}", &field.input.text[start_byte..]))
                            .style(Style::default().bg(if active { SELECTED } else { BG })),
                        input_area,
                    );
                    if active && input_area.width > 2 {
                        frame.set_cursor_position((
                            input_area.x
                                + 1
                                + UnicodeWidthStr::width(
                                    &field.input.text[start_byte..field.input.cursor],
                                ) as u16,
                            input_area.y,
                        ));
                    }
                }
                frame.render_widget(
                    Paragraph::new(self.notice.as_str())
                        .wrap(Wrap { trim: false })
                        .style(Style::default().fg(ACCENT)),
                    chunks[1],
                );
                frame.render_widget(
                    Paragraph::new("Tab / Shift+Tab move   Ctrl+S save   Esc cancel")
                        .style(Style::default().fg(MUTED)),
                    chunks[2],
                );
            }
            Screen::Routes {
                machine,
                selected,
                fallback,
                ..
            } => {
                let rect = modal(
                    frame,
                    area,
                    if *fallback {
                        "Connection failed — choose a route to try once"
                    } else {
                        "Choose this device's route"
                    },
                    86,
                    20,
                );
                if let Ok(machine) = self.machine(machine) {
                    let lines = machine
                        .routes
                        .iter()
                        .map(|r| {
                            ListItem::new(format!(
                                "{}  ·  {}:{}{}",
                                r.name,
                                r.host,
                                r.port,
                                if self
                                    .catalog
                                    .preferred(machine, &self.preferences)
                                    .is_some_and(|p| p.id == r.id)
                                {
                                    "  [selected]"
                                } else {
                                    ""
                                }
                            ))
                        })
                        .collect();
                    menu(
                        frame,
                        rect,
                        lines,
                        *selected,
                        &format!(
                            "{}\nEnter {}  A add  E edit  D delete  Esc back",
                            self.notice,
                            if *fallback { "try once" } else { "use route" }
                        ),
                    );
                }
            }
            Screen::Groups { selected } => {
                let rect = modal(frame, area, "Browse groups", 70, 23);
                let mut items = vec![ListItem::new("All machines"), ListItem::new("Ungrouped")];
                items.extend(
                    organization::groups(&self.snapshot.machines)
                        .into_iter()
                        .map(|(path, count)| {
                            ListItem::new(format!(
                                "{}{} ({count})",
                                "  ".repeat(path.matches('/').count()),
                                path.rsplit('/').next().unwrap_or(&path)
                            ))
                        }),
                );
                menu(
                    frame,
                    rect,
                    items,
                    *selected,
                    &format!("{}\nEnter browse  E rename  Esc back", self.notice),
                );
            }
            Screen::Import {
                scan,
                selected,
                marked,
                group,
                ..
            } => {
                let rect = modal(frame, area, "Import SSH hosts", 100, 27);
                let items = scan
                    .entries
                    .iter()
                    .map(|e| {
                        ListItem::new(vec![
                            Line::from(format!(
                                "[{}] {}  ·  {}@{}:{}",
                                if marked.contains(&e.alias) { "x" } else { " " },
                                e.alias,
                                e.user,
                                e.host,
                                e.port
                            )),
                            Line::styled(
                                format!("    {}", ssh_import::status(&self.snapshot.machines, e)),
                                Style::default().fg(MUTED),
                            ),
                        ])
                    })
                    .collect();
                menu(
                    frame,
                    rect,
                    items,
                    *selected,
                    &format!(
                        "{}\n{}  ·  Group: {}\nSpace mark  Ctrl+A all  Enter import  C file  G group  Esc back",
                        self.notice,
                        scan.config.display(),
                        if group.is_empty() { "Ungrouped" } else { group }
                    ),
                );
            }
            Screen::Favorite { target, .. } => {
                let rect = modal(
                    frame,
                    area,
                    &format!("Favorite: {}", self.name(target)),
                    70,
                    18,
                );
                let mut lines: Vec<_> = (1..=9)
                    .map(|n| {
                        Line::from(format!(
                            "{n}  {}",
                            self.favorites
                                .slots
                                .get(&n.to_string())
                                .map(|id| self.name(id))
                                .unwrap_or_else(|| "Empty".into())
                        ))
                    })
                    .collect();
                lines.extend([
                    Line::from(""),
                    Line::from("Press 1–9 to save here. 0 clears this item's number."),
                    Line::from("Esc back"),
                    Line::from(self.notice.clone()),
                ]);
                frame.render_widget(Paragraph::new(lines), rect);
            }
            Screen::Confirm { message, .. } => {
                let rect = modal(frame, area, "Confirm deletion", 72, 10);
                frame.render_widget(
                    Paragraph::new(format!(
                        "{message}\n\nY delete   N / Esc cancel\n{}",
                        self.notice
                    ))
                    .wrap(Wrap { trim: false }),
                    rect,
                );
            }
            Screen::Sync => {
                let rect = modal(frame, area, "Sync the catalog", 72, 12);
                frame.render_widget(Paragraph::new("P  Pull updates from the catalog's Git repository\n\nU  Save and publish the catalog\n\nEsc  Back").wrap(Wrap { trim:false }),rect);
            }
            Screen::Busy => {
                let rect = modal(frame, area, "Syncing", 64, 7);
                frame.render_widget(
                    Paragraph::new(self.notice.as_str()).wrap(Wrap { trim: false }),
                    rect,
                );
            }
            Screen::Help => {
                let rect = modal(frame, area, "Keyboard guide", 86, 25);
                frame.render_widget(Paragraph::new("↑ ↓ / Enter     Choose and open a session\n1–9 / Enter      Select a numbered favorite, then connect\nF                Assign or clear a favorite number\nG                Browse groups; E renames a selected group\n/                Search words, tag:gpu or group:Work\nSpace / Ctrl+A   Select one / all shown machines\nM / T            Move selected machines / edit tags\nA / E / D        Add / edit / delete a machine\nR                Choose, add or edit connection routes\nI                Preview local SSH hosts and import selected entries\nS                Pull or publish the catalog through Git\nF5               Reload changes from another tab\nCtrl+L           Open a local shell; exit returns to this picker\nQ                Close the picker\n\nForms: Tab moves fields, Ctrl+S saves, Esc cancels.\nFavorites apply across groups. Digits in forms remain text.\n\nEsc / F1 / Enter  Back").wrap(Wrap { trim:false }),rect);
            }
        }
    }
}
fn modal(frame: &mut Frame<'_>, area: Rect, title: &str, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let rect = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(Style::default().fg(ACCENT))
        .style(Style::default().bg(BG).fg(TEXT));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    inner
}
fn menu(
    frame: &mut Frame<'_>,
    rect: Rect,
    items: Vec<ListItem<'_>>,
    selected: usize,
    footer: &str,
) {
    let areas = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(footer.lines().count().max(2) as u16),
    ])
    .split(rect);
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().bg(SELECTED))
            .highlight_symbol("› "),
        areas[0],
        &mut ListState::default().with_selected(Some(selected)),
    );
    frame.render_widget(
        Paragraph::new(footer)
            .style(Style::default().fg(ACCENT))
            .wrap(Wrap { trim: false }),
        areas[1],
    );
}
