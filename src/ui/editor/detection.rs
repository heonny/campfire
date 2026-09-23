use super::*;

impl EditorForm {
    /// Only inspect the selected directory, once per path change; never run project code.
    pub(super) fn refresh_detection(&mut self) {
        let cwd = self.cwd.trim().to_string();
        if cwd == self.detected_for {
            return;
        }
        let dir = expand_home(&cwd);
        let has_dir = !cwd.is_empty() && dir.is_dir();
        self.cwd_exists = cwd.is_empty() || has_dir;
        self.detected = has_dir.then(|| detect_node_project(&dir)).flatten();
        self.detected_cargo = has_dir
            .then(|| crate::cargo_project::detect(&dir))
            .flatten();
        self.detected_go = has_dir.then(|| crate::go_project::detect(&dir)).flatten();
        if self.gradle_file.is_empty() || self.gradle_file == self.gradle_file_auto {
            let located = has_dir.then(|| gradle::find_build_file(&dir)).flatten();
            self.gradle_file = located.as_deref().map(collapse_home).unwrap_or_default();
            self.gradle_file_auto = self.gradle_file.clone();
        }
        self.refresh_gradle();
        let name = has_dir
            .then(|| dir.file_name())
            .flatten()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        apply_automatic(&mut self.name, &mut self.auto_name, name);
        let command = self.default_command(&dir);
        self.detection_note = self.detection_summary(command.is_some());
        apply_automatic(
            &mut self.command,
            &mut self.auto_command,
            command.unwrap_or_default(),
        );
        self.env_candidates = if has_dir {
            session::env_candidates(&dir)
        } else {
            Vec::new()
        };
        self.script_query.clear();
        self.task_query.clear();
        self.cargo_query.clear();
        self.detected_for = cwd;
    }

    fn detected_kinds(&self) -> Vec<&'static str> {
        let mut kinds = Vec::new();
        if self.detected.is_some() {
            kinds.push("Node");
        }
        if self.detected_gradle.is_some() {
            kinds.push("Gradle");
        }
        if self.detected_cargo.is_some() {
            kinds.push("Rust / Cargo");
        }
        if self.detected_go.is_some() {
            kinds.push("Go");
        }
        kinds
    }

    fn default_command(&self, dir: &std::path::Path) -> Option<String> {
        if self.detected_kinds().len() != 1 {
            return None;
        }
        if let Some(node) = &self.detected {
            return ["dev", "start"]
                .into_iter()
                .find(|script| {
                    node.scripts
                        .iter()
                        .any(|(name, raw)| name == script && !raw.trim().is_empty())
                })
                .map(|script| node.manager.run(script));
        }
        if let Some(gradle) = &self.detected_gradle {
            // A manually selected build file may belong to another module.
            if self.gradle_file != self.gradle_file_auto {
                return None;
            }
            return ["bootRun", "run"]
                .into_iter()
                .find(|task| gradle.tasks.iter().any(|t| t.name == *task))
                .map(|task| gradle::task_command(dir, task));
        }
        if let Some(go) = &self.detected_go {
            return go.runnable.then(|| "go run .".to_owned());
        }
        self.detected_cargo
            .as_ref()
            .and_then(|cargo| cargo.default_command.clone())
    }

    fn detection_summary(&self, has_default: bool) -> String {
        let kinds = self.detected_kinds();
        if kinds.len() > 1 {
            return format!(
                "Detected {}. Choose a script or target below.",
                kinds.join(" + ")
            );
        }
        if let Some(cargo) = &self.detected_cargo
            && let Some(note) = cargo.note
        {
            return note.to_owned();
        }
        if self.detected_go.as_ref().is_some_and(|go| !go.runnable) {
            return "Go module detected, but no unconditional root main found. Enter a command such as go run ./cmd/server, with build flags if needed.".into();
        }
        match kinds.first() {
            Some(kind) if has_default => {
                format!("Detected {kind}. Default command available below.")
            }
            Some(kind) => format!(
                "Detected {kind}, but no default run command. Select a task or enter a command."
            ),
            None if self.cwd.trim().is_empty() => String::new(),
            None => "No supported launch configuration found. Enter a command manually.".into(),
        }
    }
}

fn apply_automatic(value: &mut String, previous: &mut Option<String>, suggestion: String) {
    if previous.as_ref() == Some(value) {
        *value = suggestion.clone();
        *previous = Some(suggestion);
    } else {
        *previous = None;
    }
}
