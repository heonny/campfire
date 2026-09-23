use super::{detection_tests::ProjectDir, *};

fn select(dir: &ProjectDir, form: &mut EditorForm) {
    form.cwd = dir.0.to_string_lossy().into_owned();
    form.refresh_detection();
}

#[test]
fn go_module_with_main_autofills_a_savable_command() {
    let dir = ProjectDir::new(&[
        ("go.mod", "module pulse\n\ngo 1.26\n"),
        ("server.go", "package main\n\nfunc main() {}"),
    ]);
    let mut form = EditorForm::new_server();
    select(&dir, &mut form);
    assert_eq!(form.to_config().unwrap().command, "go run .");
    assert!(form.detection_note.contains("Go"));
    assert!(form.port.is_empty());
    assert!(form.env_file.is_empty());
}

#[test]
fn go_libraries_nested_targets_and_conditional_sources_require_manual_commands() {
    for (filename, source) in [
        ("library.go", "package library\nfunc main() {}"),
        ("main.go", "package main\nfunc helper() {}"),
        ("main_test.go", "package main\nfunc main() {}"),
        ("_main.go", "package main\nfunc main() {}"),
        (".main.go", "package main\nfunc main() {}"),
        ("cmd/api/main.go", "package main\nfunc main() {}"),
        (
            "main.go",
            "//go:build tools\n\npackage main\nfunc main() {}",
        ),
        ("main.go", "// +build tools\n\npackage main\nfunc main() {}"),
        ("main_windows.go", "package main\nfunc main() {}"),
        ("main_linux_amd64.go", "package main\nfunc main() {}"),
        (
            "main.go",
            "/* package main\nfunc main() {} */\npackage library",
        ),
        (
            "main.go",
            "package main\nvar example = `\nfunc main() {}\n`",
        ),
    ] {
        let dir = ProjectDir::new(&[("go.mod", "module sample\n"), (filename, source)]);
        let mut form = EditorForm::new_server();
        select(&dir, &mut form);
        assert!(
            form.command.is_empty(),
            "must not infer from {filename}: {source}"
        );
        assert!(form.detection_note.contains("Go"));
    }
}

#[test]
fn go_detection_requires_a_module_and_does_not_choose_over_node() {
    for files in [
        vec![("main.go", "package main\nfunc main() {}")],
        vec![
            ("go.mod", "module sample"),
            ("main.go", "package main\nfunc main() {}"),
            ("package.json", r#"{"scripts":{"dev":"vite"}}"#),
        ],
    ] {
        let dir = ProjectDir::new(&files);
        let mut form = EditorForm::new_server();
        select(&dir, &mut form);
        assert!(form.command.is_empty());
    }
}

#[test]
fn go_folder_changes_preserve_manual_commands_and_saved_settings() {
    let go = ProjectDir::new(&[
        ("go.mod", "module pulse"),
        ("main.go", "package main\nfunc main() {}"),
    ]);
    let node = ProjectDir::new(&[("package.json", r#"{"scripts":{"dev":"vite"}}"#)]);
    let mut form = EditorForm::new_server();
    select(&go, &mut form);
    assert_eq!(form.command, "go run .");
    select(&node, &mut form);
    assert_eq!(form.command, "npm run dev");
    form.command = "go run . --config local.toml".into();
    select(&go, &mut form);
    assert_eq!(form.command, "go run . --config local.toml");
    let config = form.to_config().unwrap();
    let mut saved = EditorForm::from_config(&config);
    saved.refresh_detection();
    assert_eq!(saved.to_config().unwrap(), config);
    assert!(!saved.is_dirty());
}
