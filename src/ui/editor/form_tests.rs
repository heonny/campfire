use super::{detection_tests::ProjectDir, *};
use eframe::egui;

fn render(
    ctx: &egui::Context,
    form: &mut EditorForm,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, EditorOutcome) {
    let mut outcome = EditorOutcome::None;
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 900.0),
            )),
            modifiers: egui::Modifiers::COMMAND,
            events,
            ..Default::default()
        },
        |ui| {
            outcome = show(ui, form, &[], false);
        },
    );
    (output, outcome)
}

fn save_key() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::COMMAND,
    }
}

#[test]
fn single_cargo_binary_is_shown_as_selected_for_the_default_command() {
    let dir = ProjectDir::new(&[
        ("Cargo.toml", "[package]\nname='api'\nversion='0.1.0'"),
        ("src/main.rs", ""),
    ]);
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let mut form = EditorForm::new_server();
    form.cwd = dir.0.to_string_lossy().into_owned();
    render(&ctx, &mut form, vec![]);
    let (output, _) = render(&ctx, &mut form, vec![]);
    assert!(
        output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Text(t) if t.galley.text() == "api"))
    );
}

#[test]
fn selecting_a_folder_in_the_form_can_save_the_detected_command() {
    let dir = ProjectDir::new(&[(
        "package.json",
        r#"{"packageManager":"pnpm@10","scripts":{"dev":"next dev"}}"#,
    )]);
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let mut form = EditorForm::new_server();
    form.cwd = dir.0.to_string_lossy().into_owned();
    render(&ctx, &mut form, vec![]);
    let (_, outcome) = render(&ctx, &mut form, vec![save_key()]);
    let EditorOutcome::Save(config) = outcome else {
        panic!("detected folder must be savable")
    };
    assert_eq!(config.command, "pnpm run dev");
    assert_eq!(config.cwd, dir.0);
    assert_eq!(config.port, None);
}

#[test]
fn go_folder_can_be_saved_from_the_form_without_a_preset() {
    let dir = ProjectDir::new(&[
        ("go.mod", "module pulse"),
        ("main.go", "package main\nfunc main() {}"),
    ]);
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let mut form = EditorForm::new_server();
    form.cwd = dir.0.to_string_lossy().into_owned();
    render(&ctx, &mut form, vec![]);
    let (_, outcome) = render(&ctx, &mut form, vec![save_key()]);
    let EditorOutcome::Save(config) = outcome else {
        panic!("Go folder must be savable")
    };
    assert_eq!(config.command, "go run .");
    assert_eq!(config.preset, Preset::Custom);
}

#[test]
fn ambiguous_cargo_form_shows_choices_and_blocks_save_until_selected() {
    let dir = ProjectDir::new(&[
        ("Cargo.toml", "[package]\nname='api'\nversion='0.1.0'"),
        ("src/bin/api.rs", ""),
        ("src/bin/worker.rs", ""),
    ]);
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    let mut form = EditorForm::new_server();
    form.cwd = dir.0.to_string_lossy().into_owned();
    render(&ctx, &mut form, vec![]);
    let (output, outcome) = render(&ctx, &mut form, vec![save_key()]);
    assert!(matches!(outcome, EditorOutcome::None));
    assert_eq!(form.error.as_deref(), Some("Command is required."));
    let choice = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "Select…" => {
                Some(t.pos + egui::vec2(8.0, 4.0))
            }
            _ => None,
        })
        .expect("Cargo target picker should be visible");
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        pressed,
        button: egui::PointerButton::Primary,
        modifiers: egui::Modifiers::NONE,
    };
    render(
        &ctx,
        &mut form,
        vec![egui::Event::PointerMoved(choice), button(choice, true)],
    );
    render(&ctx, &mut form, vec![button(choice, false)]);
    let (popup, _) = render(&ctx, &mut form, vec![]);
    let target = popup
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(t) if t.galley.text() == "worker" => {
                Some(t.pos + egui::vec2(8.0, 4.0))
            }
            _ => None,
        })
        .expect("worker target should be selectable");
    render(
        &ctx,
        &mut form,
        vec![egui::Event::PointerMoved(target), button(target, true)],
    );
    render(&ctx, &mut form, vec![button(target, false)]);
    let (_, outcome) = render(&ctx, &mut form, vec![save_key()]);
    let EditorOutcome::Save(config) = outcome else {
        panic!("selected target must be savable")
    };
    assert_eq!(config.command, "cargo run --bin worker");
}
