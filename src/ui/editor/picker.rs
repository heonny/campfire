use eframe::egui;

/// A bounded selector: commands can be arbitrarily long without widening the popup.
pub(super) fn show(
    ui: &mut egui::Ui,
    id: &str,
    current: Option<&str>,
    query: &mut String,
    choices: &[(&str, &str)],
) -> Option<usize> {
    let mut picked = None;
    let width = ui.available_width().clamp(160.0, 380.0);
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.unwrap_or("Select…"))
        .width(width)
        .height(260.0)
        .wrap_mode(egui::TextWrapMode::Truncate)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.add(
                egui::TextEdit::singleline(query)
                    .hint_text("Search name or command…")
                    .desired_width(width),
            );
            let needle = query.trim().to_lowercase();
            let mut count = 0;
            for (index, &(name, description)) in choices.iter().enumerate() {
                if !needle.is_empty()
                    && !name.to_lowercase().contains(&needle)
                    && !description.to_lowercase().contains(&needle)
                {
                    continue;
                }
                count += 1;
                let response = ui.add_sized(
                    [width, 28.0],
                    egui::Button::new(name)
                        .selected(current == Some(name))
                        .wrap_mode(egui::TextWrapMode::Truncate),
                );
                ui.add(
                    egui::Label::new(egui::RichText::new(description).small().weak()).truncate(),
                )
                .on_hover_text(description);
                if response.clicked() {
                    picked = Some(index);
                    ui.close();
                }
            }
            if count == 0 {
                ui.weak("No matching commands.");
            }
            ui.weak(format!("{count} of {} commands", choices.len()));
        });
    if let Some((_, description)) = choices.iter().find(|(name, _)| Some(*name) == current) {
        ui.add(egui::Label::new(egui::RichText::new(*description).small().weak()).truncate())
            .on_hover_text(*description);
    }
    picked
}
