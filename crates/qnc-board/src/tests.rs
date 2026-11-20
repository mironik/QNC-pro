use super::*;

const NAMES: BoardNames<'static> = BoardNames { monitor: "monitor", head: "head", body: "body", right: "right", dock: "dock" };

fn sizes() -> BoardSizes {
    BoardSizes { left_ratio: 0.31, divider_width: 5.0, left_min_width: 280.0, right_min_width: 200.0, monitor_height: 1.0, head_height: 1.0, dock_height: 1.0, monitor_aspect: None }
}

fn solved(width: f32, height: f32, sizes: BoardSizes) -> Solved {
    solve(&sizes.standard_layout(&NAMES), Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(width, height)))
}

fn face(solved: &Solved, name: &str) -> Rect {
    solved.faces.iter().find(|(face, _)| face == name).map(|(_, rect)| *rect).unwrap()
}

#[test]
fn project_places_fill_the_board_around_one_pixel_places() {
    let s = solved(1000.0, 700.0, sizes());
    let block = |name| s.block(name).unwrap();
    assert_eq!(block("dock").height(), 1.0);
    assert_eq!(block("monitor").height(), 1.0);
    assert_eq!(block("head").height(), 1.0);
    assert_eq!(block("body").height(), 700.0 - 3.0);
    assert_eq!(face(&s, "left").width(), 310.0);
    assert_eq!(face(&s, "divider").width(), 5.0);
    assert_eq!(block("right").left(), 315.0);
    assert_eq!(block("right").right(), 1000.0);
    let faces: Vec<&str> = s.faces.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(faces, ["bg", "left", "divider", "right"]);
    let blocks: Vec<&str> = s.blocks.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(blocks, ["monitor", "head", "body", "right", "dock"]);
}

#[test]
fn the_dock_never_takes_more_than_its_share_columns_keep_their_minimum_and_a_monitor_keeps_its_shape() {
    let s = solved(400.0, 300.0, BoardSizes { dock_height: 500.0, ..sizes() });
    assert!((s.block("dock").unwrap().height() - 300.0 * DOCK_MAX_SHARE).abs() < 0.01);
    assert_eq!(face(&s, "left").width(), 280.0);
    let aspect = MonitorAspect { ratio: 16.0 / 9.0, reserve_below: 190.0, min_height: 160.0, width_inset: 32.0, min_width: 240.0 };
    let s = solved(1000.0, 900.0, BoardSizes { left_ratio: 0.365, monitor_aspect: Some(aspect), head_height: 34.0, ..sizes() });
    assert!((s.block("monitor").unwrap().height() - 333.0 * 9.0 / 16.0).abs() < 0.01);
    assert_eq!(s.block("head").unwrap().top(), s.block("monitor").unwrap().bottom());
    let s = solved(1000.0, 400.0, BoardSizes { monitor_height: 900.0, head_height: 30.0, ..sizes() });
    assert_eq!(s.block("monitor").unwrap().bottom(), face(&s, "left").bottom());
    assert_eq!(s.block("body").unwrap().height(), 0.0);
}

#[test]
fn an_empty_name_leaves_a_place_empty_and_a_layout_is_data() {
    let names = BoardNames { monitor: "", head: "", body: "list", right: "settings", dock: "" };
    let s = solve(&sizes().standard_layout(&names), Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1000.0, 700.0)));
    let blocks: Vec<&str> = s.blocks.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(blocks, ["list", "settings"]);
    let layout = sizes().standard_layout(&NAMES);
    let json = serde_json::to_string(&layout).unwrap();
    assert_eq!(serde_json::from_str::<Layout>(&json).unwrap(), layout);
    let other: Layout = serde_json::from_str(
        r#"{"root":{"split":{"axis":"horizontal","parts":[
            {"size":"rest","node":{"block":{"name":"body"}}},
            {"size":{"px":{"px":400.0,"max_share":null}},"node":{"block":{"name":"monitor"}}}
        ]}}}"#,
    )
    .unwrap();
    let s = solve(&other, Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1000.0, 500.0)));
    assert_eq!(s.block("monitor").unwrap().left(), 600.0);
    assert_eq!(s.block("body").unwrap().width(), 600.0);
}

#[test]
fn every_block_is_drawn_even_after_an_earlier_one_answered() {
    let ctx = egui::Context::default();
    let mut drawn = Vec::new();
    let mut answer = None;
    let _ = ctx.run(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            answer = show_layout(ui, &sizes().standard_layout(&NAMES), |_| None, |_, name, _| {
                drawn.push(name.to_string());
                (name == "right" || name == "dock").then(|| name.to_string())
            });
        });
    });
    assert!(drawn.iter().any(|name| name == "dock"), "{drawn:?}");
    assert_eq!(answer.as_deref(), Some("right"));
}

#[test]
fn a_place_split_gives_each_panel_its_own_place_and_a_picture_across_is_as_wide_as_its_height_asks() {
    let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), Vec2::new(1000.0, 700.0));
    let split = solve(&sizes().standard_layout(&NAMES).split_place("head", "transport", 200.0), rect);
    let (head, transport) = (split.block("head").unwrap(), split.block("transport").unwrap());
    assert_eq!((head.left(), transport.right(), transport.width(), head.right()), (0.0, 310.0, 200.0, transport.left()));
    assert_eq!(sizes().standard_layout(&NAMES).split_place("nowhere", "transport", 200.0), sizes().standard_layout(&NAMES));
    let s = solve(&Layout::block("panel").with_face("right").split_place_above("panel", "bar", 40.0), rect);
    let (bar, panel) = (s.block("bar").unwrap(), s.block("panel").unwrap());
    assert_eq!((bar.top(), bar.height(), panel.top(), panel.bottom()), (0.0, 40.0, 40.0, 700.0));
    let s = solve(&Layout::block("panel").split_place_below("panel", "bar", 30.0), rect);
    assert_eq!(s.block("bar").unwrap().top(), 670.0);
    let parts = vec![Part { size: Size::Across { ratio: 16.0 / 9.0 }, face: None, node: block("monitor") }, Part { size: Size::Rest, face: None, node: block("pool") }];
    assert_eq!(solve::part_lengths(&parts, 1600.0, 360.0), vec![640.0, 960.0]);
    assert_eq!(solve::part_lengths(&parts, 500.0, 360.0), vec![500.0, 0.0]);
}
