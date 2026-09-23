use super::{EditorForm, EditorOutcome, Field, picker};
use crate::fs_util::{collapse_home, expand_home};
use crate::model::{Preset, ServerConfig};
use crate::ui::{
    frameless_edit, modal_scroll, primary_button, text_button, text_input, text_input_frame,
};
use eframe::egui;

pub fn show(
    ui: &mut egui::Ui,
    form: &mut EditorForm,
    servers: &[ServerConfig],
    self_running: bool,
) -> EditorOutcome {
    let before = form.snapshot();
    let mut outcome = EditorOutcome::None;
    let save_key = ui.input_mut(|i| {
        i.consume_shortcut(&egui::KeyboardShortcut::new(
            egui::Modifiers::COMMAND,
            egui::Key::Enter,
        ))
    });
    ui.set_width(500.0_f32.min(ui.ctx().content_rect().width() - 72.0));
    ui.heading(if form.editing_id.is_some() {
        "Edit project"
    } else {
        "Add project"
    });
    if self_running {
        ui.weak("Changes apply the next time this project starts or restarts.");
    } else {
        ui.weak("Choose a folder to detect its default run command.");
    }
    ui.add_space(8.0);
    let body_height = (ui.ctx().content_rect().height() - 235.0).clamp(160.0, 520.0);
    modal_scroll(ui)
        .max_height(body_height)
        .min_scrolled_height(body_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add_enabled_ui(!form.discard_requested, |ui| {
                input(ui, form, Field::Name, "Project name", "my-project");
                let cwd_focused = input(
                    ui,
                    form,
                    Field::Cwd,
                    "Working directory",
                    "Choose a folder…",
                );
                if !cwd_focused {
                    form.refresh_detection();
                }
                if !form.detection_note.is_empty() {
                    ui.add(egui::Label::new(&form.detection_note).wrap());
                }
                ui.horizontal(|ui| {
                    ui.label("Preset");
                    let mut chosen = form.preset;
                    egui::ComboBox::from_id_salt("preset")
                        .selected_text(chosen.label())
                        .show_ui(ui, |ui| {
                            for preset in Preset::ALL {
                                ui.selectable_value(&mut chosen, preset, preset.label());
                            }
                        });
                    if chosen != form.preset {
                        form.apply_preset(chosen);
                    }
                });
                commands(ui, form);
                input(ui, form, Field::Command, "Command", "npm run dev");
                input(ui, form, Field::Port, "Port (optional)", "3000");
                form.refresh_port();
                if let Some((hint, false)) = form.port_hint(servers, self_running) {
                    ui.colored_label(ui.visuals().warn_fg_color, hint);
                }
                ui.add_space(4.0);
                egui::CollapsingHeader::new("Environment & shell")
                    .default_open(
                        !form.env_file.is_empty() || !form.env.is_empty() || !form.shell.is_empty(),
                    )
                    .show(ui, |ui| advanced(ui, form));
                egui::CollapsingHeader::new("Launch summary").show(ui, |ui| {
                    ui.weak("Approximate command; values from environment files are not shown.");
                    crate::theme::inset_frame().show(ui, |ui| {
                        ui.add(
                            egui::Label::new(egui::RichText::new(form.preview()).monospace())
                                .wrap(),
                        );
                    });
                    ui.add(egui::Label::new(format!("Directory: {}", form.cwd)).wrap());
                    if !form.env_file.is_empty() {
                        ui.add(
                            egui::Label::new(format!("Environment file: {}", form.env_file)).wrap(),
                        );
                    }
                });
            });
        });
    if before != form.snapshot() {
        form.error = None;
    }
    ui.add_space(8.0);
    ui.allocate_ui(egui::vec2(ui.available_width(), 70.0), |ui| {
        if form.discard_requested {
            ui.label("Discard your unsaved changes?");
            ui.horizontal(|ui| {
                if ui.add(primary_button("Keep editing")).clicked() {
                    form.discard_requested = false;
                }
                if ui.add(text_button("Discard changes")).clicked() {
                    outcome = EditorOutcome::Cancel;
                }
            });
            return;
        }
        if let Some(error) = &form.error {
            ui.add(egui::Label::new(egui::RichText::new(error).color(crate::theme::DANGER)).wrap());
        }
        ui.horizontal(|ui| {
            if let Some(id) = &form.editing_id
                && ui.add(text_button("Delete project…")).clicked()
            {
                outcome = EditorOutcome::Delete(id.clone());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(primary_button("Save")).clicked() || save_key {
                    form.validation_attempted = true;
                    form.refresh_detection();
                    match form.to_config() {
                        Ok(config) => outcome = EditorOutcome::Save(config),
                        Err(error) => {
                            form.error = Some(error);
                            form.focus_error = true;
                        }
                    }
                }
                if ui.add(text_button("Cancel")).clicked() {
                    outcome = form.request_close();
                }
            });
        });
    });
    outcome
}

fn input(ui: &mut egui::Ui, form: &mut EditorForm, field: Field, label: &str, hint: &str) -> bool {
    ui.label(label);
    let focus = form.focus_error && form.first_invalid_field() == Some(field);
    let invalid = form.field_error(field).is_some();
    let value = match field {
        Field::Name => &mut form.name,
        Field::Cwd => &mut form.cwd,
        Field::Command => &mut form.command,
        Field::Port => &mut form.port,
    };
    let response = ui
        .horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if field == Field::Cwd && ui.button("Browse…").clicked() {
                    let dialog = rfd::FileDialog::new().set_directory(expand_home(value));
                    if let Some(path) = dialog.pick_folder() {
                        *value = collapse_home(&path);
                    }
                }
                let frame = text_input_frame(invalid);
                let width = (ui.available_width() - frame.total_margin().sum().x).max(50.0);
                frame
                    .show(ui, |ui| {
                        ui.add(
                            frameless_edit(value, hint, width)
                                .id_salt(("editor", format!("{field:?}"))),
                        )
                    })
                    .inner
            })
            .inner
        })
        .inner;
    if focus {
        response.request_focus();
        response.scroll_to_me(Some(egui::Align::Center));
        form.focus_error = false;
    }
    if response.changed() {
        match field {
            Field::Name => form.auto_name = None,
            Field::Command => form.auto_command = None,
            _ => {}
        }
    }
    if let Some(error) = form.field_error(field) {
        ui.colored_label(crate::theme::DANGER, error);
    }
    ui.add_space(3.0);
    response.has_focus()
}

fn commands(ui: &mut egui::Ui, form: &mut EditorForm) {
    gradle_commands(ui, form);
    node_commands(ui, form);
    cargo_commands(ui, form);
}

fn gradle_commands(ui: &mut egui::Ui, form: &mut EditorForm) {
    if form.preset == Preset::SpringBoot || !form.gradle_file.is_empty() {
        ui.label("Gradle build file");
        let (focused, browse) = path_input(ui, &mut form.gradle_file, "build.gradle");
        if browse {
            let mut dialog = rfd::FileDialog::new();
            if let Some(dir) = form.gradle_dialog_dir() {
                dialog = dialog.set_directory(dir);
            }
            if let Some(path) = dialog.pick_file() {
                form.gradle_file = collapse_home(&path);
            }
        }
        if !focused {
            form.refresh_gradle();
        }
        if let Some(project) = &form.detected_gradle
            && !project.tasks.is_empty()
        {
            ui.label("Gradle task");
            let current = project.tasks.iter().find(|t| {
                form.command == crate::gradle::task_command(&expand_home(&form.cwd), &t.name)
            });
            let choices: Vec<_> = project
                .tasks
                .iter()
                .map(|t| (t.name.as_str(), t.description.as_str()))
                .collect();
            if let Some(index) = picker::show(
                ui,
                "tasks",
                current.map(|t| t.name.as_str()),
                &mut form.task_query,
                &choices,
            ) {
                form.command = crate::gradle::task_command(
                    &expand_home(&form.cwd),
                    &project.tasks[index].name,
                );
                form.auto_command = None;
                if form.port.trim().is_empty() {
                    form.port = project.port_hint.map(|p| p.to_string()).unwrap_or_default();
                }
            }
        }
    }
}

fn node_commands(ui: &mut egui::Ui, form: &mut EditorForm) {
    if let Some(project) = &form.detected
        && !project.scripts.is_empty()
    {
        ui.label(format!("Scripts · {}", project.manager.as_str()));
        let current = project
            .scripts
            .iter()
            .find(|(name, _)| form.command == project.manager.run(name));
        let choices: Vec<_> = project
            .scripts
            .iter()
            .map(|(n, raw)| (n.as_str(), raw.as_str()))
            .collect();
        if let Some(index) = picker::show(
            ui,
            "scripts",
            current.map(|(n, _)| n.as_str()),
            &mut form.script_query,
            &choices,
        ) {
            form.command = project.manager.run(&project.scripts[index].0);
            form.auto_command = None;
            if form.port.trim().is_empty() {
                form.port = project.port_hint.map(|p| p.to_string()).unwrap_or_default();
            }
        }
    }
}

fn cargo_commands(ui: &mut egui::Ui, form: &mut EditorForm) {
    if let Some(project) = &form.detected_cargo
        && !project.commands.is_empty()
    {
        ui.label("Cargo binary");
        let choices: Vec<_> = project
            .commands
            .iter()
            .map(|(name, command)| (name.as_str(), command.as_str()))
            .collect();
        let current = project
            .commands
            .iter()
            .find(|(_, command)| command == &form.command)
            .or_else(|| {
                (project.commands.len() == 1
                    && project.default_command.as_deref() == Some(form.command.as_str()))
                .then(|| &project.commands[0])
            });
        if let Some(index) = picker::show(
            ui,
            "cargo-binaries",
            current.map(|(name, _)| name.as_str()),
            &mut form.cargo_query,
            &choices,
        ) {
            form.command = project.commands[index].1.clone();
            form.auto_command = None;
        }
    }
}

fn path_input(ui: &mut egui::Ui, value: &mut String, hint: &str) -> (bool, bool) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let browse = ui.button("Browse…").clicked();
            let width = (ui.available_width() - 18.0).max(50.0);
            (text_input(ui, value, hint, width).has_focus(), browse)
        })
        .inner
    })
    .inner
}

fn advanced(ui: &mut egui::Ui, form: &mut EditorForm) {
    ui.label("Environment file (optional)");
    let (_, browse) = path_input(ui, &mut form.env_file, ".env");
    if browse {
        let mut dialog = rfd::FileDialog::new().set_directory(expand_home(&form.cwd));
        if let Some(name) = expand_home(&form.env_file).file_name() {
            dialog = dialog.set_file_name(name.to_string_lossy());
        }
        if let Some(path) = dialog.pick_file() {
            form.env_file = collapse_home(&path);
        }
    }
    if !form.env_candidates.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.weak("Found in folder:");
            for path in &form.env_candidates {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                if ui.button(name.as_ref()).clicked() {
                    form.env_file = collapse_home(path);
                }
            }
        });
    }
    #[cfg(target_os = "macos")]
    ui.weak("In Browse, press ⌘⇧. to show hidden files.");
    ui.add_space(6.0);
    ui.label("Shell (optional)");
    let width = (ui.available_width() - 18.0).max(50.0);
    text_input(ui, &mut form.shell, "Default login shell", width);
    ui.add_space(6.0);
    ui.label("Environment variables");
    let mut remove = None;
    for (index, (key, value)) in form.env.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            let width = ((ui.available_width() - 82.0) / 2.0).max(45.0);
            text_input(ui, key, "KEY", width);
            text_input(ui, value, "Value", width);
            if ui.button("−").on_hover_text("Remove variable").clicked() {
                remove = Some(index);
            }
        });
    }
    if let Some(index) = remove {
        form.env.remove(index);
    }
    if ui.button("Add variable").clicked() {
        form.env.push((String::new(), String::new()));
    }
}
