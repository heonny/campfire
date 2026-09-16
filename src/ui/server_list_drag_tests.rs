use super::*;

fn text_position(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Pos2 {
    fn find(shape: &egui::Shape, text: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(t) if t.galley.text() == text => Some(t.pos),
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
            _ => None,
        }
    }
    shapes
        .iter()
        .find_map(|s| find(&s.shape, text))
        .unwrap_or_else(|| panic!("card text {text} disappeared during rendering"))
}

#[test]
fn dragging_into_dock_keeps_cards_stationary_and_splits_other_project() {
    drag_case(
        "Second",
        egui::pos2(1000.0, 350.0),
        &["First", "Second"],
        None,
    );
}

#[test]
fn dragging_existing_project_focuses_without_splitting() {
    drag_case("First", egui::pos2(1000.0, 350.0), &["First"], None);
}

#[test]
fn dragging_inside_sidebar_reorders_only_on_release() {
    drag_case("Second", egui::pos2(100.0, 650.0), &["First"], Some((1, 2)));
}

#[test]
fn dropping_outside_panels_cancels_without_reordering() {
    drag_case("Second", egui::pos2(-20.0, 350.0), &["First"], None);
}

fn drag_case(
    source: &str,
    destination: egui::Pos2,
    expected: &[&str],
    reorder: Option<(usize, usize)>,
) {
    let ctx = egui::Context::default();
    crate::theme::setup(&ctx);
    egui_extras::install_image_loaders(&ctx);
    let servers: Vec<_> = ["First", "Second", "Third"]
        .into_iter()
        .map(|name| {
            let mut s = ServerConfig::from_preset(name, ".", crate::model::Preset::Custom);
            s.id = name.to_owned();
            s
        })
        .collect();
    let metrics = crate::metrics::Metrics::new();
    let view = View {
        active: 0,
        servers: &servers,
        running: &Default::default(),
        dup_ports: &Default::default(),
        focused: Some("First"),
        metrics: &metrics,
    };
    let mut workspaces = crate::ui::workspaces::Workspaces::new();
    workspaces.active_mut().open_auto("First");
    let mut dock = None;
    let mut action = None;
    let mut render = |events: Vec<egui::Event>| {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1100.0, 700.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                let drag = egui::Panel::left("test-sidebar")
                    .exact_size(240.0)
                    .show(ui, |ui| show(ui, &view, &mut action, dock))
                    .inner;
                dock = Some(
                    egui::CentralPanel::default()
                        .show(ui, |ui| workspaces.show(ui, &view, &mut action, &drag).0)
                        .inner,
                );
            },
        )
    };
    render(vec![]);
    let baseline = render(vec![]);
    let origin = text_position(&baseline.shapes, source) + egui::vec2(10.0, 4.0);
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    render(vec![
        egui::Event::PointerMoved(origin),
        button(origin, true),
    ]);
    render(vec![egui::Event::PointerMoved(
        origin + egui::vec2(12.0, 0.0),
    )]);
    let dragging = render(vec![egui::Event::PointerMoved(destination)]);
    for name in ["First", "Second", "Third"] {
        assert_eq!(
            text_position(&baseline.shapes, name),
            text_position(&dragging.shapes, name),
            "sidebar cards must stay in place during dragging"
        );
    }
    fn is_warning(shape: &egui::Shape) -> bool {
        match shape {
            egui::Shape::Rect(rect) => rect.stroke.color == egui::Color32::RED,
            egui::Shape::Vec(shapes) => shapes.iter().any(is_warning),
            _ => false,
        }
    }
    assert!(
        !dragging.shapes.iter().any(|s| is_warning(&s.shape)),
        "unstable widget IDs produced a red diagnostic rectangle"
    );
    render(vec![button(destination, false)]);
    assert_eq!(workspaces.active().open_ids(), expected);
    match (action, reorder) {
        (Some(Action::Reorder { from, to }), Some(expected)) => assert_eq!((from, to), expected),
        (None, None) => {}
        _ => panic!("unexpected sidebar action"),
    }
}
