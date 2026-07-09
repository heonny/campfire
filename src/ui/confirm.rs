//! A confirmation modal for a destructive action. Currently used only for
//! deleting a server — a one-click, irreversible operation (it stops the
//! process and permanently removes the saved config), so both entry points
//! (the sidebar context menu and the editor's Delete button) route through it.
//! The caller owns the pending state; this view only reports the user's choice.

use super::text_button;
use crate::theme;
use eframe::egui;

/// The user's choice in the confirm dialog for one frame.
pub enum ConfirmOutcome {
    /// No button pressed yet — the dialog stays open.
    None,
    /// Dismissed without confirming. The caller also maps click-away / Esc here.
    Cancel,
    /// Confirmed the destructive action.
    Confirm,
}

/// Show a "delete this server?" confirmation for the server named `name`. The
/// confirm button is filled with the error color (and sits on the right, macOS
/// style) so the destructive result is unmistakable; Cancel is plain.
pub fn show_delete(ui: &mut egui::Ui, name: &str) -> ConfirmOutcome {
    ui.set_max_width(360.0);
    ui.heading("Delete server?");
    ui.add_space(4.0);
    ui.label(format!(
        "'{name}' will be stopped and permanently removed. This can't be undone."
    ));
    ui.add_space(12.0);

    let mut outcome = ConfirmOutcome::None;
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let delete = egui::Button::new(egui::RichText::new("Delete").color(egui::Color32::WHITE))
            .fill(theme::DANGER);
        if ui.add(delete).clicked() {
            outcome = ConfirmOutcome::Confirm;
        }
        if ui.add(text_button("Cancel")).clicked() {
            outcome = ConfirmOutcome::Cancel;
        }
    });
    outcome
}
