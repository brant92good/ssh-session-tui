use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use ssh_sessions::{
    catalog::Catalog,
    connection,
    favorites::{Favorites, LOCAL},
    organization, ssh_import,
    sync::GitSync,
    ui,
};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "ssh-sessions",
    version,
    about = "Browse, organize and connect to your SSH machines."
)]
struct Args {
    #[arg(long, global = true)]
    catalog: Option<PathBuf>,
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Action>,
}
#[derive(Subcommand)]
enum Action {
    /// List machines and this device's selected routes.
    List {
        #[arg(long)]
        json: bool,
        #[arg(long, conflicts_with = "ungrouped")]
        group: Option<String>,
        #[arg(long)]
        ungrouped: bool,
        #[arg(long)]
        tag: Option<String>,
        #[arg(long, default_value = "")]
        search: String,
    },
    /// Create an empty catalog if missing.
    Init,
    /// Check local setup without making changes.
    Doctor {
        #[arg(long)]
        json: bool,
    },
    /// Pull or publish the catalog through Git.
    Sync {
        #[arg(value_parser = ["pull", "publish"])]
        action: String,
        #[arg(long)]
        json: bool,
    },
    /// Print an SSH argument list without opening a session.
    Command {
        machine: String,
        #[arg(long)]
        route: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Preview SSH hosts. Import only with --apply and --host NAME or --all.
    ImportSsh {
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long, conflicts_with = "all")]
        host: Vec<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        apply: bool,
        #[arg(long, default_value = "")]
        group: String,
        #[arg(long)]
        json: bool,
    },
    /// List or edit numbered favorites.
    Favorites {
        #[arg(default_value = "list", value_parser = ["list", "set", "remove"])]
        action: String,
        slot: Option<String>,
        #[arg(long, conflicts_with = "local")]
        machine: Option<String>,
        #[arg(long)]
        local: bool,
        #[arg(long)]
        json: bool,
    },
    /// List groups or rename a group and its subgroups.
    Groups {
        #[arg(default_value = "list", value_parser = ["list", "rename"])]
        action: String,
        old: Option<String>,
        new: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Move selected machines and add or remove tags.
    Organize {
        #[arg(long, required = true)]
        machine: Vec<String>,
        #[arg(long, conflicts_with = "ungrouped")]
        group: Option<String>,
        #[arg(long)]
        ungrouped: bool,
        #[arg(long)]
        add_tag: Vec<String>,
        #[arg(long)]
        remove_tag: Vec<String>,
        #[arg(long)]
        json: bool,
    },
}
fn emit(value: Value, structured: bool, text: String) {
    if structured {
        println!("{value}");
    } else {
        println!("{text}");
    }
}
fn group_result(catalog: &Catalog, structured: bool) -> Result<()> {
    let snapshot = catalog.load()?;
    let rows: Vec<_> = organization::groups(&snapshot.machines)
        .into_iter()
        .map(|(path, count)| json!({"path":path,"machines":count}))
        .collect();
    let text = rows
        .iter()
        .map(|r| {
            format!(
                "{} ({})",
                r["path"].as_str().unwrap_or_default(),
                r["machines"]
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    emit(
        json!({"ok":true,"groups":rows,"ungrouped":snapshot.machines.iter().filter(|m| m.group.is_empty()).count()}),
        structured,
        if text.is_empty() {
            "No groups yet. Use organize --group to create one.".into()
        } else {
            text
        },
    );
    Ok(())
}
fn run(args: Args) -> Result<()> {
    let directory = connection::default_directory();
    let catalog = Catalog::new(
        &args
            .catalog
            .unwrap_or_else(|| directory.join("catalog.json")),
        &args.state_dir.unwrap_or_else(|| directory.join("device")),
    )?;
    let snapshot = catalog.load()?;
    match args.command {
        None => ui::picker_loop(&catalog)?,
        Some(Action::Init) => {
            if !catalog.path.exists() {
                catalog.save(&[], &snapshot.revision)?;
            }
            println!("Catalog ready: {}", catalog.path.display());
        }
        Some(Action::List {
            json: structured,
            group,
            ungrouped,
            tag,
            search,
        }) => {
            let preferences = catalog.preferences()?;
            let mut rows = Vec::new();
            let mut lines = Vec::new();
            for machine in organization::filtered(
                &snapshot.machines,
                &search,
                if ungrouped {
                    Some("")
                } else {
                    group.as_deref()
                },
                tag.as_deref(),
            ) {
                let route = catalog.preferred(machine, &preferences);
                let mut row = serde_json::to_value(machine)?;
                row["group"] = json!(machine.group);
                row["tags"] = json!(machine.tags);
                row["selected_route"] = json!(route.map(|r| &r.id));
                if let Some(routes) = row["routes"].as_array_mut() {
                    for r in routes {
                        if r.get("ssh_alias").is_none() {
                            r["ssh_alias"] = Value::Null;
                        }
                    }
                }
                lines.push(format!(
                    "{} | {} | {}",
                    machine.name,
                    route
                        .map(|r| format!("{}: {}", r.name, r.host))
                        .unwrap_or_else(|| "Choose a route".into()),
                    machine.user
                ));
                rows.push(row);
            }
            emit(
                json!({"ok":true,"version":1,"machines":rows}),
                structured,
                if lines.is_empty() {
                    "No machines yet. Run without a command and press A to add one.".into()
                } else {
                    lines.join("\n")
                },
            );
        }
        Some(Action::Doctor { json: structured }) => {
            catalog.preferences()?;
            Favorites::load(&catalog)?;
            let ssh = which::which("ssh").is_ok();
            let checks = json!({"catalog_valid":true,"machine_count":snapshot.machines.len(),"ssh_available":ssh,"git_available":which::which("git").is_ok(),"local_pwsh_available":which::which("pwsh").is_ok(),"local_shell":connection::local_shell_name(),"runtime":"rust"});
            emit(
                json!({"ok":ssh,"checks":checks,"next_step":"Run without a command to choose or add a machine. Git is needed only for sync."}),
                structured,
                serde_json::to_string_pretty(&checks)?,
            );
            if !ssh {
                std::process::exit(1);
            }
        }
        Some(Action::Command {
            machine,
            route,
            json: structured,
        }) => {
            let matches: Vec<_> = snapshot
                .machines
                .iter()
                .filter(|m| m.id == machine || m.name == machine)
                .collect();
            ensure!(
                matches.len() == 1,
                "Select a unique machine ID or exact name."
            );
            let machine = matches[0];
            let preferences = catalog.preferences()?;
            let selected = if let Some(route) = route {
                let routes: Vec<_> = machine
                    .routes
                    .iter()
                    .filter(|r| r.id == route || r.name == route)
                    .collect();
                ensure!(routes.len() == 1, "Select a unique route ID or name.");
                Some(routes[0])
            } else {
                catalog.preferred(machine, &preferences)
            }
            .context("Choose a route explicitly with --route or in the TUI.")?;
            let config = ssh_import::connection_config(&catalog, machine, selected)?;
            let args = connection::ssh_command(machine, selected, config.as_deref())?;
            emit(
                json!({"ok":true,"argv":args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>()}),
                structured,
                connection::display_command(&args),
            );
        }
        Some(Action::ImportSsh {
            config,
            host,
            all,
            apply,
            group,
            json: structured,
        }) => {
            let preview = ssh_import::scan(config.as_deref())?;
            let mut rows = Vec::new();
            let mut lines = Vec::new();
            for entry in &preview.entries {
                let status = ssh_import::status(&snapshot.machines, entry);
                let mut row = serde_json::to_value(entry)?;
                row["status"] = json!(status);
                lines.push(format!(
                    "{} | {}@{}:{} | {}",
                    entry.alias, entry.user, entry.host, entry.port, status
                ));
                rows.push(row);
            }
            let mut result =
                json!({"ok":true,"source":preview.config,"hosts":rows,"applied":false});
            if apply {
                let aliases = if all {
                    preview
                        .entries
                        .iter()
                        .filter(|e| {
                            matches!(
                                ssh_import::status(&snapshot.machines, e).as_str(),
                                ssh_import::READY | ssh_import::IMPORTED
                            )
                        })
                        .map(|e| e.alias.clone())
                        .collect()
                } else {
                    host
                };
                let imported =
                    ssh_import::import(&catalog, &preview, &aliases, &snapshot.revision, &group)?;
                for (key, value) in serde_json::to_value(imported)?
                    .as_object()
                    .context("Invalid import result")?
                {
                    result[key] = value.clone();
                }
                result["applied"] = json!(true);
                lines.push("Hosts imported.".into());
            } else {
                lines.push("Use --apply with --host NAME or --all to import.".into());
            }
            emit(result, structured, lines.join("\n"));
        }
        Some(Action::Favorites {
            action,
            slot,
            machine,
            local,
            json: structured,
        }) => {
            match action.as_str() {
                "set" => {
                    let target = if local {
                        Some(LOCAL)
                    } else {
                        machine.as_deref()
                    }
                    .context("Use --machine MACHINE_ID or --local.")?;
                    Favorites::assign(
                        &catalog,
                        slot.as_deref().context("Choose a favorite number.")?,
                        Some(target),
                        None,
                    )?;
                }
                "remove" => {
                    ensure!(
                        machine.is_none() && !local,
                        "Use favorites remove NUMBER without a target."
                    );
                    Favorites::assign(
                        &catalog,
                        slot.as_deref().context("Choose a favorite number.")?,
                        None,
                        None,
                    )?;
                }
                _ => ensure!(
                    slot.is_none() && machine.is_none() && !local,
                    "Use favorites list without a slot or target."
                ),
            }
            let rows: Vec<_> = Favorites::load(&catalog)?.0.slots.into_iter().map(|(slot,target)| { let machine = snapshot.machines.iter().find(|m| m.id == target); let name = if target == LOCAL { "Local terminal" } else { machine.map(|m| m.name.as_str()).unwrap_or("Missing machine") }; json!({"slot":slot.parse::<u8>().unwrap_or(0),"target":target,"name":name,"target_exists":target == LOCAL || machine.is_some()}) }).collect();
            let text = rows
                .iter()
                .map(|r| format!("{}: {}", r["slot"], r["name"].as_str().unwrap_or_default()))
                .collect::<Vec<_>>()
                .join("\n");
            emit(
                json!({"ok":true,"favorites":rows,"scope":"device","activation":"number_then_enter"}),
                structured,
                if text.is_empty() {
                    "No numbered favorites. Select a row and press F in the picker.".into()
                } else {
                    text
                },
            );
        }
        Some(Action::Groups {
            action,
            old,
            new,
            json: structured,
        }) => {
            if action == "rename" {
                organization::rename_group(
                    &catalog,
                    old.as_deref().context("Use groups rename OLD NEW.")?,
                    new.as_deref().context("Use groups rename OLD NEW.")?,
                    &snapshot.revision,
                )?;
            } else {
                ensure!(
                    old.is_none() && new.is_none(),
                    "Use groups list without path arguments."
                );
            }
            group_result(&catalog, structured)?;
        }
        Some(Action::Organize {
            machine,
            group,
            ungrouped,
            add_tag,
            remove_tag,
            json: structured,
        }) => {
            let group = if ungrouped {
                Some("")
            } else {
                group.as_deref()
            };
            ensure!(
                group.is_some() || !add_tag.is_empty() || !remove_tag.is_empty(),
                "Specify --group, --add-tag or --remove-tag."
            );
            organization::edit_many(
                &catalog,
                &machine,
                &snapshot.revision,
                group,
                &add_tag,
                &remove_tag,
            )?;
            group_result(&catalog, structured)?;
        }
        Some(Action::Sync {
            action,
            json: structured,
        }) => {
            let message = GitSync::new(&catalog)?.run(&action)?;
            emit(json!({"ok":true,"message":message}), structured, message);
        }
    }
    Ok(())
}
fn main() {
    let structured = std::env::args_os().any(|a| a == "--json");
    if let Err(error) = run(Args::parse()) {
        if structured {
            println!("{}", json!({"ok":false,"error":format!("{error:#}")}));
        } else {
            eprintln!("SSH Sessions: {error:#}");
        }
        std::process::exit(1);
    }
}
