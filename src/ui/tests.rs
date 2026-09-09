use super::*;
use ratatui::backend::TestBackend;
fn press(picker: &mut Picker, code: KeyCode) {
    picker.key(KeyEvent::new(code, KeyModifiers::NONE));
}
fn type_text(picker: &mut Picker, text: &str) {
    for c in text.chars() {
        press(picker, KeyCode::Char(c));
    }
}
fn fixture() -> (tempfile::TempDir, Picker) {
    let temp = tempfile::tempdir().unwrap();
    let catalog = Catalog::new(
        &temp.path().join("catalog.json"),
        &temp.path().join("device"),
    )
    .unwrap();
    let machines = ["lab", "school"]
        .into_iter()
        .map(|name| Machine {
            id: name.into(),
            name: format!("{name} 開發"),
            user: "dev".into(),
            group: name.into(),
            tags: vec!["gpu".into()],
            routes: vec![
                Route {
                    id: "lan".into(),
                    name: "LAN".into(),
                    host: "192.0.2.10".into(),
                    port: 22,
                    ssh_alias: None,
                },
                Route {
                    id: "vpn".into(),
                    name: "VPN".into(),
                    host: "gpu.example.test".into(),
                    port: 22,
                    ssh_alias: None,
                },
            ],
        })
        .collect::<Vec<_>>();
    catalog
        .save(&machines, &catalog.load().unwrap().revision)
        .unwrap();
    Favorites::assign(&catalog, "1", Some("lab"), None).unwrap();
    Favorites::assign(&catalog, "2", Some(LOCAL), None).unwrap();
    (temp, Picker::new(catalog).unwrap())
}
fn render(picker: &Picker, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| picker.draw(frame)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
}
#[test]
fn favorite_selects_across_groups_then_enter_requires_a_route() {
    let (_temp, mut picker) = fixture();
    picker.group = Some("school".into());
    press(&mut picker, KeyCode::Char('1'));
    assert_eq!(picker.current(), "lab");
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(
        picker.screen,
        Screen::Routes {
            connect_after: true,
            ..
        }
    ));
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Down);
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(picker.choice,Some(Choice::Connect(_,ref r)) if r.id=="vpn"));
    assert_eq!(picker.catalog.preferences().unwrap()["lab"], "vpn");
}
#[test]
fn fallback_is_explicit_and_does_not_change_preference() {
    let (_temp, mut picker) = fixture();
    picker
        .catalog
        .choose(picker.machine("lab").unwrap(), "lan")
        .unwrap();
    picker.screen = Screen::Routes {
        machine: "lab".into(),
        selected: 0,
        fallback: true,
        connect_after: true,
    };
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Down);
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(picker.choice,Some(Choice::Connect(_,ref r)) if r.id=="vpn"));
    assert_eq!(picker.catalog.preferences().unwrap()["lab"], "lan");
}
#[test]
fn local_favorite_and_modal_numeric_isolation() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('2'));
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(picker.choice.take(), Some(Choice::Local)));
    press(&mut picker, KeyCode::Char('a'));
    type_text(&mut picker, "Server 123");
    assert!(picker.choice.is_none());
    if let Screen::Form(form) = &picker.screen {
        assert_eq!(form.fields[0].input.text, "Server 123");
    } else {
        panic!("Expected form");
    }
    press(&mut picker, KeyCode::Esc);
    press(&mut picker, KeyCode::Char('/'));
    type_text(&mut picker, "123");
    assert_eq!(picker.query.text, "123");
    assert!(picker.choice.is_none());
}
#[test]
fn invalid_number_then_enter_cannot_open_the_prior_or_adjacent_row() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Char('9'));
    press(&mut picker, KeyCode::Enter);
    assert!(picker.choice.is_none());
    assert!(matches!(picker.screen, Screen::Main));
    let snapshot = picker.catalog.load().unwrap();
    picker
        .catalog
        .save(&snapshot.machines[1..], &snapshot.revision)
        .unwrap();
    picker.reload().unwrap();
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Enter);
    assert!(picker.choice.is_none());
    press(&mut picker, KeyCode::Esc);
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(picker.choice, Some(Choice::Local)));
}
#[test]
fn changed_catalog_requires_review_before_connecting_from_list_or_route_modal() {
    let (_temp, mut picker) = fixture();
    picker
        .catalog
        .choose(&picker.snapshot.machines[0], "lan")
        .unwrap();
    picker.reload().unwrap();
    press(&mut picker, KeyCode::Char('1'));
    let mut snapshot = picker.catalog.load().unwrap();
    snapshot.machines[0].routes[0].host = "192.0.2.90".into();
    picker
        .catalog
        .save(&snapshot.machines, &snapshot.revision)
        .unwrap();
    press(&mut picker, KeyCode::Enter);
    assert!(picker.choice.is_none());
    assert!(picker.notice.contains("changed"));
    press(&mut picker, KeyCode::Enter);
    assert!(
        matches!(picker.choice.take(), Some(Choice::Connect(_,route)) if route.host=="192.0.2.90")
    );
    press(&mut picker, KeyCode::Char('r'));
    snapshot = picker.catalog.load().unwrap();
    snapshot.machines[0].routes[0].host = "192.0.2.91".into();
    picker
        .catalog
        .save(&snapshot.machines, &snapshot.revision)
        .unwrap();
    press(&mut picker, KeyCode::Enter);
    assert!(picker.choice.is_none());
    assert!(matches!(picker.screen, Screen::Main));
}
#[test]
fn favorite_reassignment_and_stale_slot_menu() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('f'));
    Favorites::assign(&picker.catalog, "4", Some("school"), None).unwrap();
    press(&mut picker, KeyCode::Char('4'));
    assert!(!picker.notice.contains("changed"));
    press(&mut picker, KeyCode::Enter);
    assert!(picker.notice.contains("changed"));
    assert_eq!(
        Favorites::load(&picker.catalog).unwrap().0.slots["4"],
        "school"
    );
}
#[test]
fn favorite_slot_selection_can_cancel_and_clear_a_missing_target() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('f'));
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Esc);
    assert_eq!(
        Favorites::load(&picker.catalog).unwrap().0.slots["1"],
        "lab"
    );
    let snapshot = picker.catalog.load().unwrap();
    picker
        .catalog
        .save(&snapshot.machines[1..], &snapshot.revision)
        .unwrap();
    picker.reload().unwrap();
    press(&mut picker, KeyCode::Char('f'));
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Char('d'));
    assert!(
        !Favorites::load(&picker.catalog)
            .unwrap()
            .0
            .slots
            .contains_key("1")
    );
}
#[test]
fn import_enter_uses_highlighted_alias_and_a_marks_all() {
    let (temp, mut picker) = fixture();
    let config = temp.path().join("import-config");
    std::fs::write(
        &config,
        "Host alpha beta\n HostName 192.0.2.44\n User dev\n",
    )
    .unwrap();
    picker.screen = Screen::Import {
        scan: ssh_import::scan(Some(&config)).unwrap(),
        selected: 1,
        marked: BTreeSet::new(),
        group: "Imported".into(),
        revision: picker.snapshot.revision.clone(),
    };
    press(&mut picker, KeyCode::Enter);
    assert!(matches!(picker.screen, Screen::Main), "{}", picker.notice);
    assert!(picker.catalog.load().unwrap().machines.iter().any(|m| {
        m.routes
            .iter()
            .any(|r| r.ssh_alias.as_deref() == Some("beta"))
    }));
    assert!(!picker.catalog.load().unwrap().machines.iter().any(|m| {
        m.routes
            .iter()
            .any(|r| r.ssh_alias.as_deref() == Some("alpha"))
    }));
    picker.screen = Screen::Import {
        scan: ssh_import::scan(Some(&config)).unwrap(),
        selected: 0,
        marked: BTreeSet::new(),
        group: String::new(),
        revision: picker.snapshot.revision.clone(),
    };
    press(&mut picker, KeyCode::Char('a'));
    assert!(matches!(&picker.screen, Screen::Import { marked,.. } if marked.len()==2));
}
#[test]
fn form_unicode_edit_save_and_stale_save_protection() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Char('e'));
    picker.key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
    type_text(&mut picker, "台中 GPU");
    press(&mut picker, KeyCode::Left);
    press(&mut picker, KeyCode::Backspace);
    type_text(&mut picker, "X");
    picker.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert_eq!(picker.catalog.load().unwrap().machines[0].name, "台中 GXU");
    press(&mut picker, KeyCode::Char('e'));
    type_text(&mut picker, " stale");
    let snapshot = picker.catalog.load().unwrap();
    let mut other = snapshot.machines;
    other[0].name = "Other tab".into();
    picker.catalog.save(&other, &snapshot.revision).unwrap();
    picker.key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));
    assert!(picker.notice.contains("changed"));
    assert_eq!(picker.catalog.load().unwrap().machines[0].name, "Other tab");
}
#[test]
fn search_clears_bulk_selection_and_deleted_favorite_cannot_select_neighbor() {
    let (_temp, mut picker) = fixture();
    press(&mut picker, KeyCode::Char('1'));
    press(&mut picker, KeyCode::Char(' '));
    assert!(!picker.marked.is_empty());
    press(&mut picker, KeyCode::Char('/'));
    assert!(picker.marked.is_empty());
    press(&mut picker, KeyCode::Esc);
    let snapshot = picker.catalog.load().unwrap();
    picker
        .catalog
        .save(&snapshot.machines[1..], &snapshot.revision)
        .unwrap();
    picker.reload().unwrap();
    press(&mut picker, KeyCode::Char('1'));
    assert!(picker.notice.contains("missing"));
    assert!(picker.choice.is_none());
}
#[test]
fn all_screens_render_at_normal_narrow_and_tiny_sizes() {
    let (_temp, mut picker) = fixture();
    assert!(render(&picker, 100, 32).contains("Local terminal"));
    for key in ['a', 'f', 'g', 'r', 'd', 's'] {
        picker.screen = Screen::Main;
        picker.selected = 1;
        press(&mut picker, KeyCode::Char(key));
        for (width, height) in [(120, 36), (60, 18), (12, 4), (1, 1)] {
            render(&picker, width, height);
        }
    }
    picker.screen = Screen::Help;
    assert!(render(&picker, 100, 32).contains("Keyboard guide"));
}
