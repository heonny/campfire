//! Detect a Gradle project from its build script: which plugins it applies and
//! which runnable tasks those plugins conventionally expose.
//!
//! This mirrors [`crate::project`] for Node — static, best-effort parsing that
//! feeds the editor's task picker only; it never touches the persisted
//! [`crate::model::ServerConfig`]. A missing or unparseable build file yields
//! `None`, never an error.
//!
//! Unlike npm scripts, Gradle tasks are contributed by plugins rather than
//! declared verbatim, so we can't read them off the file. Instead we parse the
//! applied plugin ids and map each to the conventional tasks it registers.
//! Running `gradle tasks` would be exact but spins up the daemon (seconds,
//! blocking) — wrong for a picker that refreshes as the user types.

use std::path::{Path, PathBuf};

/// The Spring Boot plugin id — the one gradle preset users configure here.
const SPRING_BOOT: &str = "org.springframework.boot";

/// A runnable Gradle task surfaced in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleTask {
    pub name: String,
    pub description: String,
}

/// A detected Gradle project: the parsed build file, its applied plugins, and
/// the tasks those plugins conventionally expose (in most-runnable-first order).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradleProject {
    pub build_file: PathBuf,
    /// Applied plugin ids in declaration order (`org.springframework.boot`,
    /// `java`, …), deduped.
    pub plugins: Vec<String>,
    /// Runnable tasks derived from `plugins`.
    pub tasks: Vec<GradleTask>,
    /// Conventional dev port for a recognized framework (Spring Boot → 8080).
    pub port_hint: Option<u16>,
}

/// The shell command that runs `task` through the Gradle wrapper, matching the
/// Spring Boot preset's `./gradlew` convention so the picker and the preset
/// default agree.
pub fn task_command(task: &str) -> String {
    format!("./gradlew {task}")
}

/// Locate a conventional build script directly under `dir`. Prefers the Groovy
/// DSL (`build.gradle`) over Kotlin (`build.gradle.kts`); only one normally
/// exists, so the order rarely matters.
pub fn find_build_file(dir: &Path) -> Option<PathBuf> {
    ["build.gradle", "build.gradle.kts"]
        .into_iter()
        .map(|name| dir.join(name))
        .find(|path| path.is_file())
}

/// Read and parse `build_file`. Returns `None` when it can't be read; a readable
/// file with no recognized plugins still yields a project with empty `tasks`
/// (the picker simply shows nothing).
pub fn detect_gradle_project(build_file: &Path) -> Option<GradleProject> {
    let text = std::fs::read_to_string(build_file).ok()?;
    let plugins = parse_plugins(&text);
    let tasks = tasks_for(&plugins);
    let port_hint = plugins.iter().any(|p| p == SPRING_BOOT).then_some(8080);
    Some(GradleProject {
        build_file: build_file.to_path_buf(),
        plugins,
        tasks,
        port_hint,
    })
}

/// Extract applied plugin ids from build-script `text` (Groovy or Kotlin DSL).
/// Handles the `plugins { … }` block (`id '…'`, `id("…")`, `kotlin("jvm")`, and
/// bare Kotlin accessors like `java`) plus legacy top-level `apply plugin: '…'`.
pub fn parse_plugins(text: &str) -> Vec<String> {
    let cleaned = strip_line_comments(text);
    let mut ids = Vec::new();

    if let Some(block) = plugins_block_inner(&cleaned) {
        for line in block.lines() {
            if let Some(id) = plugin_from_block_line(line) {
                push_unique(&mut ids, id);
            }
        }
    }

    // Legacy `apply plugin: 'java'`. Scanned across the whole script — a real
    // `plugins { }` block never uses this syntax, so it won't double-count.
    for line in cleaned.lines() {
        if let Some((_, after)) = line.split_once("apply plugin:")
            && let Some(id) = first_quoted(after)
        {
            push_unique(&mut ids, id.to_string());
        }
    }

    ids
}

/// Derive the runnable tasks for a set of applied plugins, deduped and ordered
/// most-runnable-first. Base lifecycle tasks (`build`, `test`, `check`, …) are
/// offered whenever any JVM plugin is present, since they all pull in `base`.
fn tasks_for(plugins: &[String]) -> Vec<GradleTask> {
    let has = |id: &str| plugins.iter().any(|p| p == id);
    let jvm = [
        "java",
        "java-library",
        "application",
        "war",
        "groovy",
        "scala",
        "org.jetbrains.kotlin.jvm",
        SPRING_BOOT,
    ]
    .iter()
    .any(|&id| has(id));

    let mut tasks = Vec::new();
    if has(SPRING_BOOT) {
        tasks.push(task("bootRun", "Run the Spring Boot app in place"));
        tasks.push(task("bootJar", "Build the executable Spring Boot jar"));
        tasks.push(task("bootBuildImage", "Build an OCI image via buildpacks"));
        tasks.push(task("bootTestRun", "Run the app with the test classpath"));
    }
    if has("application") {
        tasks.push(task("run", "Run the application"));
    }
    if jvm {
        tasks.push(task("build", "Assemble and test everything"));
        tasks.push(task("assemble", "Build outputs, skip tests"));
        tasks.push(task("jar", "Package the main classes as a jar"));
        tasks.push(task("test", "Run the unit tests"));
        tasks.push(task("check", "Run all verification tasks"));
        tasks.push(task("clean", "Delete the build directory"));
    }
    if has("maven-publish") {
        tasks.push(task("publish", "Publish to a remote repository"));
        tasks.push(task(
            "publishToMavenLocal",
            "Install to the local Maven repo",
        ));
    }
    tasks
}

fn task(name: &str, description: &str) -> GradleTask {
    GradleTask {
        name: name.to_string(),
        description: description.to_string(),
    }
}

fn push_unique(ids: &mut Vec<String>, id: String) {
    if !id.is_empty() && !ids.contains(&id) {
        ids.push(id);
    }
}

/// Interpret a single line inside a `plugins { … }` block as a plugin id.
fn plugin_from_block_line(line: &str) -> Option<String> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    // `kotlin("jvm")` -> `org.jetbrains.kotlin.jvm`
    if let Some(start) = line.find("kotlin(") {
        return first_quoted(&line[start..]).map(|arg| format!("org.jetbrains.kotlin.{arg}"));
    }
    // `id 'x'`, `id("x")`, `id "x" version "…"` — take the first quoted string.
    if line.contains('\'') || line.contains('"') {
        return first_quoted(line).map(str::to_string);
    }
    // A bare Kotlin accessor: `java`, `application`, `` `java-library` ``, …
    // Restrict to known core plugins so stray tokens aren't captured.
    const BARE: &[&str] = &[
        "java",
        "java-library",
        "application",
        "war",
        "groovy",
        "scala",
        "base",
    ];
    let token = line
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_matches('`');
    BARE.contains(&token).then(|| token.to_string())
}

/// The contents of the first single- or double-quoted string in `s`, if any.
fn first_quoted(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = bytes.iter().position(|&b| b == b'\'' || b == b'"')?;
    let quote = bytes[start];
    let rest = &s[start + 1..];
    let end = rest.find(quote as char)?;
    Some(&rest[..end])
}

/// Drop `//` line comments so they don't confuse plugin/block parsing. Plugin
/// ids never contain `//`, so cutting at the first occurrence is safe here.
fn strip_line_comments(text: &str) -> String {
    text.lines()
        .map(|line| match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The inner contents of the first `plugins { … }` block, brace-matched so
/// nested config closures don't end it early.
fn plugins_block_inner(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = text[from..].find("plugins") {
        let pos = from + rel;
        let after = pos + "plugins".len();
        // Reject when "plugins" is part of a larger identifier (e.g. `myplugins`).
        let joined_left =
            pos > 0 && (bytes[pos - 1].is_ascii_alphanumeric() || bytes[pos - 1] == b'_');
        let mut j = after;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if !joined_left && j < bytes.len() && bytes[j] == b'{' {
            return Some(brace_match(&text[j..]));
        }
        from = after;
    }
    None
}

/// Given a slice starting at `{`, return the contents up to the matching `}`.
/// Falls back to the remainder when braces are unbalanced.
fn brace_match(from_open: &str) -> String {
    let mut depth = 0i32;
    for (i, c) in from_open.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return from_open[1..i].to_string();
                }
            }
            _ => {}
        }
    }
    from_open[1..].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn scratch(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("campfire-gradle-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names(tasks: &[GradleTask]) -> Vec<&str> {
        tasks.iter().map(|t| t.name.as_str()).collect()
    }

    #[test]
    fn parses_groovy_plugins_block() {
        let text = r#"
            plugins {
                id 'org.springframework.boot' version '3.2.0'
                id 'io.spring.dependency-management' version '1.1.4'
                id 'java'
            }
            repositories { mavenCentral() }
        "#;
        let plugins = parse_plugins(text);
        assert_eq!(
            plugins,
            vec![
                "org.springframework.boot",
                "io.spring.dependency-management",
                "java",
            ]
        );
    }

    #[test]
    fn parses_kotlin_dsl_block() {
        let text = r#"
            plugins {
                java
                id("org.springframework.boot") version "3.2.0"
                kotlin("jvm") version "1.9.22"
            }
        "#;
        let plugins = parse_plugins(text);
        assert_eq!(
            plugins,
            vec![
                "java",
                "org.springframework.boot",
                "org.jetbrains.kotlin.jvm",
            ]
        );
    }

    #[test]
    fn parses_legacy_apply_plugin() {
        let text = "apply plugin: 'java'\napply plugin: \"org.springframework.boot\"\n";
        assert_eq!(
            parse_plugins(text),
            vec!["java", "org.springframework.boot"]
        );
    }

    #[test]
    fn ignores_commented_and_unresolvable_lines() {
        let text = r#"
            plugins {
                // id 'should.be.ignored'
                id 'java'
                alias(libs.plugins.spotless)
            }
        "#;
        // The commented id and the version-catalog alias are both dropped.
        assert_eq!(parse_plugins(text), vec!["java"]);
    }

    #[test]
    fn spring_boot_tasks_lead_with_bootrun() {
        let plugins = vec!["org.springframework.boot".to_string(), "java".to_string()];
        let tasks = tasks_for(&plugins);
        assert_eq!(names(&tasks)[0], "bootRun");
        for expected in ["bootRun", "bootJar", "build", "jar", "test", "check"] {
            assert!(
                names(&tasks).contains(&expected),
                "missing {expected} in {:?}",
                names(&tasks)
            );
        }
    }

    #[test]
    fn plain_java_has_lifecycle_but_no_boot_tasks() {
        let tasks = tasks_for(&["java".to_string()]);
        assert!(names(&tasks).contains(&"build"));
        assert!(names(&tasks).contains(&"test"));
        assert!(!names(&tasks).iter().any(|n| n.starts_with("boot")));
    }

    #[test]
    fn no_plugins_yields_no_tasks() {
        assert!(tasks_for(&[]).is_empty());
    }

    #[test]
    fn detect_reads_file_and_hints_spring_boot_port() {
        let dir = scratch("detect");
        fs::write(
            dir.join("build.gradle"),
            "plugins {\n id 'org.springframework.boot'\n id 'java'\n}\n",
        )
        .unwrap();
        let project = detect_gradle_project(&dir.join("build.gradle")).unwrap();
        assert_eq!(project.port_hint, Some(8080));
        assert_eq!(project.tasks[0].name, "bootRun");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn find_build_file_prefers_groovy_then_kotlin() {
        let dir = scratch("find");
        assert!(find_build_file(&dir).is_none());
        fs::write(dir.join("build.gradle.kts"), "plugins { java }").unwrap();
        assert_eq!(find_build_file(&dir).unwrap(), dir.join("build.gradle.kts"));
        fs::write(dir.join("build.gradle"), "plugins { id 'java' }").unwrap();
        assert_eq!(find_build_file(&dir).unwrap(), dir.join("build.gradle"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_none_when_file_absent() {
        let dir = scratch("absent");
        assert!(detect_gradle_project(&dir.join("build.gradle")).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn task_command_uses_wrapper() {
        assert_eq!(task_command("bootRun"), "./gradlew bootRun");
    }

    #[test]
    fn brace_matched_block_ignores_nested_closures() {
        let text = r#"
            plugins {
                id 'java'
                id 'com.example.thing' version '1.0'
            }
            dependencies {
                implementation('org.example:lib') { transitive = false }
            }
        "#;
        // The nested closure in `dependencies` must not leak plugin ids.
        assert_eq!(parse_plugins(text), vec!["java", "com.example.thing"]);
    }
}
