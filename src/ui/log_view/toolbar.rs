//! Search and log navigation controls, sized for split panes.
use super::{Events, LogView, ScrollTo, SearchInfo};
use crate::ui::{frameless_edit, icon_button, icons, text_input_frame};
use eframe::egui;

pub(super) fn search_rows(
    ui: &mut egui::Ui,
    state: &mut LogView,
    info: SearchInfo,
    events: &mut Events,
    focus_find: bool,
) {
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let next = ui
                .add_enabled(info.total > 0, icon_button(icons::chevron_down()))
                .on_hover_text("Next match (Enter)");
            next.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, info.total > 0, "Next match")
            });
            if next.clicked() {
                events.nav = 1;
            }
            let previous = ui
                .add_enabled(info.total > 0, icon_button(icons::chevron_up()))
                .on_hover_text("Previous match (Shift+Enter)");
            previous.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::WidgetType::Button,
                    info.total > 0,
                    "Previous match",
                )
            });
            if previous.clicked() {
                events.nav = -1;
            }
            let response = field(
                ui,
                &mut state.find,
                "Find in output",
                info.find_error.is_some(),
            );
            if focus_find {
                response.request_focus();
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                events.nav = if ui.input(|i| i.modifiers.shift) {
                    -1
                } else {
                    1
                };
                response.request_focus();
            }
        });
    });
    ui.horizontal_wrapped(|ui| {
        ui.menu_button("Options", |ui| {
            ui.checkbox(&mut state.find_case, "Match case");
            ui.checkbox(&mut state.find_word, "Whole word");
            ui.checkbox(&mut state.find_regex, "Regular expression");
        });
        if ui
            .selectable_label(
                state.filter_open,
                if state.filter_open {
                    "Remove filter"
                } else {
                    "Filter lines"
                },
            )
            .on_hover_text("Show or remove the line filter")
            .clicked()
        {
            state.filter_open = !state.filter_open;
            if !state.filter_open {
                state.grep.clear();
            }
        }
        if !state.find.is_empty() {
            let current = if info.total == 0 { 0 } else { state.active + 1 };
            ui.weak(format!("{current}/{}", info.total));
        }
    });
    if let Some(error) = info.find_error {
        ui.colored_label(crate::theme::DANGER, "Invalid search expression")
            .on_hover_text(error);
    }
    if state.filter_open {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Options", |ui| {
                    ui.checkbox(&mut state.grep_case, "Match case");
                    ui.checkbox(&mut state.grep_word, "Whole word");
                    ui.checkbox(&mut state.grep_regex, "Regular expression");
                });
                field(
                    ui,
                    &mut state.grep,
                    "Filter lines",
                    info.grep_error.is_some(),
                );
            });
        });
        if let Some(error) = info.grep_error {
            ui.colored_label(crate::theme::DANGER, "Invalid filter · showing all lines")
                .on_hover_text(error);
        } else if !state.grep.is_empty() {
            ui.weak(format!("{} matching lines", info.grep_count.unwrap_or(0)));
        }
    }
}

fn field(ui: &mut egui::Ui, value: &mut String, hint: &str, invalid: bool) -> egui::Response {
    let frame = text_input_frame(invalid);
    let width = (ui.available_width() - frame.total_margin().sum().x).max(40.0);
    frame
        .show(ui, |ui| ui.add(frameless_edit(value, hint, width)))
        .inner
}

pub(super) fn bottom_bar(ui: &mut egui::Ui, state: &mut LogView, unseen: u64, events: &mut Events) {
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(state.search_open, "Find")
            .on_hover_text("Find in output (Cmd/Ctrl+F)")
            .clicked()
        {
            events.toggle_search = true;
        }
        if ui
            .checkbox(&mut state.follow, "Follow")
            .on_hover_text("Keep new output in view")
            .changed()
            && state.follow
        {
            events.catch_up = true;
        }
        ui.menu_button("More", |ui| {
            ui.checkbox(&mut state.wrap, "Wrap long lines");
            if ui.button("Scroll to top").clicked() {
                state.scroll_to = Some(ScrollTo::Top);
                state.follow = false;
                ui.close();
            }
            if ui.button("Scroll to bottom").clicked() {
                state.scroll_to = Some(ScrollTo::Bottom);
                ui.close();
            }
            if ui.button("Clear output").clicked() {
                events.clear = true;
                ui.close();
            }
        });
        if unseen > 0 && ui.button(format!("↓ {unseen} new")).clicked() {
            events.catch_up = true;
        }
    });
}

pub(super) fn row_divider(ui: &mut egui::Ui) {
    ui.add_space(6.0);
}
