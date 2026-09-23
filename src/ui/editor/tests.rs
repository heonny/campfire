use super::*;

fn form(name: &str, port: &str) -> EditorForm {
    EditorForm {
        editing_id: None,
        name: name.to_string(),
        preset: Preset::Custom,
        cwd: std::env::temp_dir().to_string_lossy().into_owned(),
        command: "run".to_string(),
        port: port.to_string(),
        env_file: String::new(),
        env: Vec::new(),
        shell: String::new(),
        error: None,
        detected: None,
        detected_cargo: None,
        detected_go: None,
        auto_name: None,
        auto_command: None,
        detection_note: String::new(),
        cargo_query: String::new(),
        detected_for: String::new(),
        cwd_exists: true,
        port_checked: String::new(),
        port_in_use: false,
        gradle_file: String::new(),
        gradle_file_auto: String::new(),
        detected_gradle: None,
        detected_gradle_for: String::new(),
        initial_snapshot: None,
        discard_requested: false,
        validation_attempted: false,
        focus_error: false,
        script_query: String::new(),
        task_query: String::new(),
        env_candidates: Vec::new(),
    }
}

#[test]
fn home_paths_show_as_tilde_and_expand_on_save() {
    let home = directories::BaseDirs::new()
        .unwrap()
        .home_dir()
        .to_path_buf();
    let mut config = ServerConfig::from_preset("api", home.clone(), Preset::Custom);
    config.command = "run".into();
    let f = EditorForm::from_config(&config);
    assert_eq!(f.cwd, "~");
    assert_eq!(f.to_config().unwrap().cwd, home);
}

#[test]
fn to_config_rejects_a_missing_working_dir() {
    let mut f = form("ok", "");
    f.cwd = std::env::temp_dir()
        .join("campfire-no-such-dir")
        .to_string_lossy()
        .into_owned();
    assert!(f.to_config().is_err());
}

#[test]
fn port_hint_flags_parse_errors_and_config_duplicates() {
    let servers = [ServerConfig::from_preset("api", "/srv/api", Preset::NextJs)]; // :3000
    assert!(form("a", "").port_hint(&servers, false).is_none());
    assert!(form("a", "abc").port_hint(&servers, false).unwrap().1);
    let (hint, is_error) = form("a", "3000").port_hint(&servers, false).unwrap();
    assert!(hint.contains("api"), "got: {hint}");
    assert!(!is_error);
    assert!(form("a", "3001").port_hint(&servers, false).is_none());
}

#[test]
fn to_config_parses_and_empties_become_none() {
    let mut f = form("api", "3000");
    f.command = "npm run dev".to_string();
    f.env = vec![
        ("K".to_string(), "V".to_string()),
        ("  ".to_string(), "dropped".to_string()),
    ];
    let config = f.to_config().unwrap();
    assert_eq!(config.name, "api");
    assert_eq!(config.port, Some(3000));
    assert_eq!(config.env_file, None);
    assert_eq!(config.shell, None);
    assert_eq!(config.env.len(), 1); // blank-key row filtered out
    assert_eq!(config.env[0].key, "K");
    assert!(!config.id.is_empty());
}

#[test]
fn to_config_rejects_bad_input() {
    assert!(form("", "8080").to_config().is_err()); // empty name
    assert!(form("ok", "abc").to_config().is_err()); // non-numeric port
    assert!(form("ok", "0").to_config().is_err()); // port 0
    assert!(form("ok", "").to_config().is_ok()); // empty port -> None, ok
    assert!(form("ok", "8080").to_config().is_ok());
}

#[test]
fn new_form_is_clean_until_user_edits() {
    let mut f = EditorForm::new_server();
    assert!(!f.is_dirty());
    f.name.push_str("draft");
    assert!(f.is_dirty());
}

#[test]
fn detected_gradle_path_does_not_make_saved_form_dirty() {
    let dir = std::env::temp_dir().join(format!("campfire-dirty-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("build.gradle"), "plugins { id 'java' }").unwrap();
    let mut config = form("api", "").to_config().unwrap();
    config.cwd = dir.clone();
    config.preset = Preset::SpringBoot;
    let mut f = EditorForm::from_config(&config);
    f.refresh_detection();
    std::fs::remove_dir_all(dir).unwrap();
    assert!(!f.gradle_file.is_empty());
    assert!(
        !f.is_dirty(),
        "automatic detection must not block dismissal"
    );
}

#[test]
fn changed_environment_pair_is_not_equal_to_original() {
    let mut config = form("api", "").to_config().unwrap();
    config.env = vec![EnvVar {
        key: "A".into(),
        value: "B=C".into(),
    }];
    let mut f = EditorForm::from_config(&config);
    f.env = vec![("A=B".into(), "C".into())];
    assert!(f.is_dirty(), "key/value boundaries must be preserved");
}

#[test]
fn editing_id_is_preserved() {
    let mut f = form("a", "");
    f.editing_id = Some("fixed-id".to_string());
    assert_eq!(f.to_config().unwrap().id, "fixed-id");
}

#[test]
fn preview_includes_port_and_command() {
    let mut f = form("a", "3000");
    f.command = "npm run dev".to_string();
    let preview = f.preview();
    assert!(preview.contains("PORT=3000"), "got: {preview}");
    assert!(preview.contains("npm run dev"), "got: {preview}");
}

#[test]
fn launch_summary_masks_values_and_shows_both_injected_ports() {
    let mut f = form("api", "8080");
    f.env = vec![("TOKEN".into(), "private-value".into())];
    let preview = f.preview();
    assert!(!preview.contains("private-value"));
    assert!(preview.contains("SERVER_PORT=8080"));
}

/// Two sibling scratch dirs, each with a `build.gradle`, for exercising the
/// Spring Boot auto-locate / manual-override state machine.
fn gradle_dirs(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    use std::fs;
    let base = std::env::temp_dir().join(format!("campfire-editor-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let (a, b) = (base.join("a"), base.join("b"));
    for dir in [&a, &b] {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("build.gradle"), "plugins { id 'java' }").unwrap();
    }
    (a, b)
}

#[test]
fn gradle_file_auto_follows_cwd_when_not_overridden() {
    let (a, b) = gradle_dirs("follow");
    let mut f = EditorForm::new_server();
    f.preset = Preset::SpringBoot;

    f.cwd = a.to_string_lossy().into_owned();
    f.refresh_detection();
    assert_eq!(f.gradle_file, a.join("build.gradle").to_string_lossy());

    // Untouched auto value tracks the new working directory.
    f.cwd = b.to_string_lossy().into_owned();
    f.refresh_detection();
    assert_eq!(f.gradle_file, b.join("build.gradle").to_string_lossy());

    let _ = std::fs::remove_dir_all(a.parent().unwrap());
}

#[test]
fn gradle_file_manual_override_survives_cwd_change() {
    let (a, b) = gradle_dirs("override");
    let mut f = EditorForm::new_server();
    f.preset = Preset::SpringBoot;
    f.cwd = a.to_string_lossy().into_owned();
    f.refresh_detection();

    // User Browses to a specific module's build file.
    let manual = a.join("app/build.gradle").to_string_lossy().into_owned();
    f.gradle_file = manual.clone();

    // An incidental cwd edit must not clobber the manual override.
    f.cwd = b.to_string_lossy().into_owned();
    f.refresh_detection();
    assert_eq!(f.gradle_file, manual);

    let _ = std::fs::remove_dir_all(a.parent().unwrap());
}
