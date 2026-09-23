use super::*;

pub(super) struct ProjectDir(pub(super) PathBuf);

impl ProjectDir {
    pub(super) fn new(files: &[(&str, &str)]) -> Self {
        let dir = std::env::temp_dir().join(format!("campfire-autofill-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in files {
            let path = dir.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
        Self(dir)
    }

    fn select(&self, form: &mut EditorForm) {
        form.cwd = self.0.to_string_lossy().into_owned();
        form.refresh_detection();
    }
}

impl Drop for ProjectDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn folder_autofills_node_command_and_savable_name() {
    let dir = ProjectDir::new(&[(
        "package.json",
        r#"{"packageManager":"pnpm@10","scripts":{"start":"next start","dev":"next dev"}}"#,
    )]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    let config = form.to_config().unwrap();
    assert_eq!(config.command, "pnpm run dev");
    assert_eq!(config.name, dir.0.file_name().unwrap().to_str().unwrap());
    assert_eq!(
        config.port, None,
        "detection must not override the app's port"
    );
}

#[test]
fn folder_autofills_gradle_without_selecting_a_preset() {
    let dir = ProjectDir::new(&[
        (
            "build.gradle.kts",
            "plugins {\n id(\"org.springframework.boot\") version \"3.5.0\"\n}",
        ),
        ("gradlew", ""),
        ("gradlew.bat", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    #[cfg(unix)]
    assert_eq!(form.command, "./gradlew bootRun");
    #[cfg(windows)]
    assert_eq!(form.command, ".\\gradlew.bat bootRun");
}

#[test]
fn folder_autofills_cargo_binary() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname = 'api'\nversion = '0.1.0'\nedition = '2024'",
        ),
        ("src/main.rs", "fn main() {}"),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert_eq!(form.to_config().unwrap().command, "cargo run");
}

#[test]
fn folder_changes_replace_only_untouched_automatic_values() {
    let node = ProjectDir::new(&[("package.json", r#"{"scripts":{"dev":"vite"}}"#)]);
    let gradle = ProjectDir::new(&[("build.gradle", "plugins { application }")]);
    let unknown = ProjectDir::new(&[]);
    let mut form = EditorForm::new_server();
    node.select(&mut form);
    assert_eq!(form.command, "npm run dev");
    gradle.select(&mut form);
    assert_eq!(form.command, "gradle run");
    unknown.select(&mut form);
    assert!(
        form.command.is_empty(),
        "old project's command must not leak"
    );
    node.select(&mut form);
    form.command = "pnpm run dev -- --host".into();
    form.name = "My server".into();
    form.port = "4567".into();
    gradle.select(&mut form);
    assert_eq!(form.command, "pnpm run dev -- --host");
    assert_eq!(form.name, "My server");
    assert_eq!(form.port, "4567");
}

#[test]
fn saved_configuration_is_never_rewritten_by_detection() {
    let dir = ProjectDir::new(&[("package.json", r#"{"scripts":{"dev":"next dev"}}"#)]);
    let mut config = ServerConfig::from_preset("Saved", &dir.0, Preset::Custom);
    config.command = "node custom.js".into();
    let mut form = EditorForm::from_config(&config);
    form.refresh_detection();
    assert_eq!(form.to_config().unwrap(), config);
    assert!(!form.is_dirty());
}

#[test]
fn unsupported_and_ambiguous_projects_do_not_get_a_default() {
    let cases: &[&[(&str, &str)]] = &[
        &[(
            "package.json",
            r#"{"scripts":{"build":"vite build","deploy":"deploy"}}"#,
        )],
        &[("package.json", "{broken")],
        &[("build.gradle", "plugins { java }")],
        &[(
            "build.gradle",
            "plugins {\n id 'org.springframework.boot' version '3.5.0' apply false\n}",
        )],
        &[
            ("Cargo.toml", "[package]\nname='lib'\nversion='0.1.0'"),
            ("src/lib.rs", ""),
        ],
        &[("Cargo.toml", "[workspace]\nmembers=['api']")],
        &[
            ("package.json", r#"{"scripts":{"dev":"vite"}}"#),
            ("build.gradle", "plugins { application }"),
        ],
    ];
    for files in cases {
        let dir = ProjectDir::new(files);
        let mut form = EditorForm::new_server();
        dir.select(&mut form);
        assert!(form.command.is_empty(), "must not guess for {files:?}");
        assert!(form.to_config().is_err());
    }
}

#[test]
fn explicit_preset_survives_folder_detection() {
    let dir = ProjectDir::new(&[("package.json", r#"{"scripts":{"dev":"vite"}}"#)]);
    let mut form = EditorForm::new_server();
    form.apply_preset(Preset::Go);
    dir.select(&mut form);
    assert_eq!(form.command, "go run .");
    assert_eq!(form.port, "8080");
}

#[test]
fn cargo_default_run_selects_the_named_binary() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname='api'\nversion='0.1.0'\ndefault-run='server'",
        ),
        ("src/bin/server.rs", "fn main() {}"),
        ("src/bin/worker/main.rs", "fn main() {}"),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert_eq!(form.command, "cargo run --bin server");
}

#[test]
fn cargo_does_not_guess_between_binaries_or_enable_features() {
    let cases: &[&[(&str, &str)]] = &[
        &[
            ("Cargo.toml", "[package]\nname='api'\nversion='0.1.0'"),
            ("src/bin/server.rs", ""),
            ("src/bin/worker.rs", ""),
        ],
        &[
            (
                "Cargo.toml",
                "[package]\nname='api'\nversion='0.1.0'\nautobins=false",
            ),
            ("src/main.rs", ""),
        ],
        &[
            (
                "Cargo.toml",
                "[package]\nname='api'\nversion='0.1.0'\n[[bin]]\nname='server'\npath='server.rs'\nrequired-features=['web']",
            ),
            ("server.rs", ""),
        ],
    ];
    for files in cases {
        let dir = ProjectDir::new(files);
        let mut form = EditorForm::new_server();
        dir.select(&mut form);
        assert!(form.command.is_empty(), "must not guess for {files:?}");
    }
}

#[test]
fn cargo_explicit_binary_respects_autobins_false() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname='api'\nversion='0.1.0'\nautobins=false\n[[bin]]\nname='server'\npath='app/server.rs'",
        ),
        ("app/server.rs", "fn main() {}"),
        ("src/bin/ignored.rs", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert_eq!(form.command, "cargo run");
}

#[test]
fn cargo_does_not_pick_a_default_after_excluding_a_feature_gated_binary() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname='api'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='worker'\nrequired-features=['worker']",
        ),
        ("src/main.rs", ""),
        ("src/bin/worker.rs", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert!(form.command.is_empty());
    assert_eq!(
        form.detected_cargo.as_ref().unwrap().commands,
        vec![("api".into(), "cargo run --bin api".into())]
    );
}

#[test]
fn cargo_renamed_main_is_not_counted_twice() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname='api'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='server'\npath='src/main.rs'",
        ),
        ("src/main.rs", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert_eq!(form.command, "cargo run");
}

#[test]
fn cargo_workspace_root_does_not_run_a_different_default_member() {
    let dir = ProjectDir::new(&[
        (
            "Cargo.toml",
            "[package]\nname='root'\nversion='0.1.0'\n[workspace]\nmembers=['other']\ndefault-members=['other']",
        ),
        ("src/main.rs", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert_eq!(form.command, "cargo run -p root --bin root");
}

#[test]
fn commented_or_unapplied_gradle_plugins_do_not_autofill() {
    for script in [
        "plugins {\n /* id 'org.springframework.boot' */\n java\n}",
        "plugins {\n id('org.springframework.boot') version '3.5.0'\n apply false\n}",
        "plugins {\n id('org.springframework.boot') version '3.5.0' apply   false\n}",
    ] {
        let dir = ProjectDir::new(&[("build.gradle", script)]);
        let mut form = EditorForm::new_server();
        dir.select(&mut form);
        assert!(
            form.command.is_empty(),
            "must not run unapplied plugin: {script}"
        );
    }
}

#[test]
fn cargo_picker_exposes_multiple_targets_and_rejects_shell_tokens() {
    let dir = ProjectDir::new(&[
        ("Cargo.toml", "[package]\nname='api'\nversion='0.1.0'"),
        ("src/bin/server.rs", ""),
        ("src/bin/worker/main.rs", ""),
        ("src/bin/a;echo-injected.rs", ""),
    ]);
    let mut form = EditorForm::new_server();
    dir.select(&mut form);
    assert!(form.command.is_empty());
    assert_eq!(
        form.detected_cargo.as_ref().unwrap().commands,
        vec![
            ("server".into(), "cargo run --bin server".into()),
            ("worker".into(), "cargo run --bin worker".into())
        ]
    );
}

#[test]
fn node_start_and_lockfiles_choose_the_matching_runner() {
    for (lockfile, expected) in [
        ("pnpm-lock.yaml", "pnpm run start"),
        ("yarn.lock", "yarn run start"),
        ("bun.lock", "bun run start"),
        ("package-lock.json", "npm run start"),
    ] {
        let dir = ProjectDir::new(&[
            ("package.json", r#"{"scripts":{"start":"node app.js"}}"#),
            (lockfile, ""),
        ]);
        let mut form = EditorForm::new_server();
        dir.select(&mut form);
        assert_eq!(form.command, expected);
    }
}
