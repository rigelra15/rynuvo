use std::collections::HashMap;

use rynuvo_desktop_lib::prefs::{self, Prefs};

#[test]
fn prefs_default_on_missing() {
    let d = tempfile::tempdir().unwrap();
    let p = prefs::load(&d.path().join("prefs.json"));
    assert_eq!(p, Prefs::default());
    assert_eq!(p.vad_threshold, 0.5);
    assert_eq!(p.shortcut_mute, "CmdOrCtrl+Shift+M");
    assert_eq!(p.shortcut_deafen, "CmdOrCtrl+Shift+D");
    assert!(!p.autostart);
}

#[test]
fn prefs_default_on_corrupt() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("prefs.json");
    std::fs::write(&f, b"{not json").unwrap();
    assert_eq!(prefs::load(&f), Prefs::default());
}

#[test]
fn prefs_roundtrip() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("sub/prefs.json");
    let p = Prefs {
        input_device: Some("USB Mic".into()),
        output_device: None,
        vad_threshold: 0.7,
        volumes: HashMap::from([(3, 1.5)]),
        shortcut_mute: "Alt+M".into(),
        shortcut_deafen: "Alt+D".into(),
        autostart: true,
    };
    prefs::save(&f, &p).unwrap();
    assert_eq!(prefs::load(&f), p);
    assert!(!d.path().join("sub/prefs.json.tmp").exists(), "temp file renamed away");
}

#[test]
fn unknown_fields_ignored() {
    let d = tempfile::tempdir().unwrap();
    let f = d.path().join("prefs.json");
    std::fs::write(&f, br#"{"vadThreshold":0.3,"fromTheFuture":true}"#).unwrap();
    let p = prefs::load(&f);
    assert_eq!(p.vad_threshold, 0.3);
    assert_eq!(p.shortcut_mute, "CmdOrCtrl+Shift+M");
}

#[test]
fn shortcut_strings_parse() {
    use tauri_plugin_global_shortcut::Shortcut;
    let p = Prefs::default();
    for s in [p.shortcut_mute.as_str(), p.shortcut_deafen.as_str(), "CmdOrCtrl+Alt+Space", "Ctrl+Shift+F5", "Alt+Shift+0"] {
        assert!(s.parse::<Shortcut>().is_ok(), "{s}");
    }
}
