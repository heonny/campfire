use super::{EditorForm, EditorOutcome};
use crate::model::Preset;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Eq)]
pub(super) struct EditorSnapshot {
    name: String,
    preset: Preset,
    cwd: String,
    command: String,
    port: String,
    env_file: String,
    env: Vec<(String, String)>,
    shell: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Field {
    Name,
    Cwd,
    Command,
    Port,
}

impl EditorForm {
    pub(super) fn snapshot(&self) -> EditorSnapshot {
        EditorSnapshot {
            name: self.name.clone(),
            preset: self.preset,
            cwd: self.cwd.clone(),
            command: self.command.clone(),
            port: self.port.clone(),
            env_file: self.env_file.clone(),
            env: self.env.clone(),
            shell: self.shell.clone(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.initial_snapshot
            .as_ref()
            .is_none_or(|initial| *initial != self.snapshot())
    }

    pub fn request_close(&mut self) -> EditorOutcome {
        if self.is_dirty() {
            self.discard_requested = true;
            EditorOutcome::None
        } else {
            EditorOutcome::Cancel
        }
    }

    pub(super) fn field_error(&self, field: Field) -> Option<&'static str> {
        match field {
            Field::Name if self.validation_attempted && self.name.trim().is_empty() => {
                Some("Enter a project name.")
            }
            Field::Cwd if self.validation_attempted && self.cwd.trim().is_empty() => {
                Some("Choose a working directory.")
            }
            Field::Cwd if !self.cwd_exists => Some("This directory does not exist."),
            Field::Command if self.validation_attempted && self.command.trim().is_empty() => {
                Some("Enter a command or select a script.")
            }
            Field::Port
                if !self.port.trim().is_empty()
                    && !self.port.trim().parse::<u16>().is_ok_and(|p| p > 0) =>
            {
                Some("Enter a port from 1 to 65535.")
            }
            _ => None,
        }
    }

    pub(super) fn first_invalid_field(&self) -> Option<Field> {
        [Field::Name, Field::Cwd, Field::Command, Field::Port]
            .into_iter()
            .find(|field| self.field_error(*field).is_some())
    }
}

pub(super) fn env_candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name == ".env" || name.starts_with(".env.")) && entry.path().is_file()
        })
        .map(|entry| entry.path())
        .collect();
    paths.sort();
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_dirty_draft_keeps_values_until_explicit_discard() {
        let mut form = EditorForm::new_server();
        assert!(matches!(form.request_close(), EditorOutcome::Cancel));
        form.name = "draft".into();
        assert!(matches!(form.request_close(), EditorOutcome::None));
        assert!(form.discard_requested);
        assert_eq!(form.name, "draft");
        form.discard_requested = false;
        assert!(form.is_dirty());
    }

    #[test]
    fn required_error_clears_as_its_field_is_corrected() {
        let mut form = EditorForm::new_server();
        form.validation_attempted = true;
        assert_eq!(form.first_invalid_field(), Some(Field::Name));
        form.name = "project".into();
        assert!(form.field_error(Field::Name).is_none());
        assert_eq!(form.first_invalid_field(), Some(Field::Cwd));
        form.port = "70000".into();
        assert!(form.field_error(Field::Port).is_some());
        form.port = "8080".into();
        assert!(form.field_error(Field::Port).is_none());
    }

    #[test]
    fn env_candidates_include_hidden_files_but_not_directories() {
        let dir = std::env::temp_dir().join(format!("campfire-env-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".env.folder")).unwrap();
        for name in [".env.local", ".env", "not-env"] {
            std::fs::write(dir.join(name), "").unwrap();
        }
        let candidates = env_candidates(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(candidates, vec![dir.join(".env"), dir.join(".env.local")]);
    }
}
