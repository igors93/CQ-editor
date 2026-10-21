use super::*;

fn node(key: &str, name: &str, children: Vec<SceneNode>) -> SceneNode {
    SceneNode {
        key: key.into(),
        name: name.into(),
        kind: "Workplane".into(),
        inspectable: false,
        parts: vec![Part {
            mesh: Arc::new(Mesh::lines(vec![[0.0; 3], [1.0, 2.0, 3.0]])),
            color: None,
            alpha: None,
        }],
        children,
        visible: true,
        color: None,
        alpha: None,
        edge_color: None,
        overrides: Overrides::default(),
    }
}

fn scene() -> Scene {
    let mut scene = Scene::default();
    scene.append(
        vec![
            node(
                "a",
                "assy",
                vec![node("a1", "plate", vec![]), node("a2", "pin", vec![])],
            ),
            node("b", "box", vec![]),
        ],
        None,
    );
    scene
}

#[test]
fn hiding_a_parent_hides_the_subtree() {
    let mut scene = scene();
    scene.set_visible("a", false);
    let visible: Vec<_> = scene
        .visible_nodes()
        .iter()
        .map(|n| n.key.as_str())
        .collect();
    assert_eq!(visible, ["b"]);
}

#[test]
fn selection_roots_skip_nested_selections() {
    let mut scene = scene();
    scene.select_only("a2");
    scene.toggle_selected("a");
    scene.toggle_selected("b");
    assert_eq!(scene.selected_roots(), ["a", "b"]);
    assert_eq!(scene.selected_top_level(), ["a", "b"]);
    assert_eq!(scene.current().unwrap().key, "b");
}

#[test]
fn properties_survive_a_reload_by_name() {
    let mut scene = scene();
    scene.set_visible("a1", false);
    scene.find_mut("b").unwrap().overrides.color = Some([1.0, 0.0, 0.0]);
    let preserved = scene.snapshot_properties();

    let removed = scene.replace(
        vec![
            node("x", "assy", vec![node("x1", "plate", vec![])]),
            node("y", "box", vec![]),
        ],
        Some(&preserved),
    );
    assert_eq!(removed, ["a", "b"]);
    assert!(!scene.find("x1").unwrap().visible);
    assert_eq!(
        scene.find("y").unwrap().overrides.color,
        Some([1.0, 0.0, 0.0])
    );
}

#[test]
fn colors_resolve_in_priority_order() {
    let mut n = node("n", "n", vec![]);
    let default = [0.5; 3];
    assert_eq!(n.part_color(&n.parts[0], default), (default, 1.0));
    n.color = Some([0.0, 0.0, 1.0]);
    n.alpha = Some(0.5);
    assert_eq!(n.part_color(&n.parts[0], default), ([0.0, 0.0, 1.0], 0.5));
    n.overrides.color = Some([1.0, 0.0, 0.0]);
    assert_eq!(n.part_color(&n.parts[0], default).0, [1.0, 0.0, 0.0]);
}

#[test]
fn axis_helpers_scale_with_the_model() {
    let mut scene = Scene::default();
    assert_eq!(scene.axis_half_length(), DEFAULT_AXIS_HALF_LENGTH);
    scene.append(vec![node("b", "box", vec![])], None);
    let diagonal = Vec3::new(1.0, 2.0, 3.0).length();
    assert!((scene.axis_half_length() - 2.0 * diagonal).abs() < 1e-5);
}

#[test]
fn bounding_boxes_include_all_points_and_ignore_empty_boxes() {
    let box_a = Aabb::from_points([Vec3::new(-2.0, 1.0, 3.0), Vec3::new(4.0, 5.0, 7.0)]);
    assert_eq!(Aabb::EMPTY.union(box_a), box_a);
    assert_eq!(box_a.center(), Vec3::new(1.0, 3.0, 5.0));
    assert_eq!(box_a.size(), Vec3::new(6.0, 4.0, 4.0));
    assert!(Aabb::from_points([]).is_empty());
}

#[test]
fn mesh_bounds_include_edges_and_free_points() {
    let mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0]],
        vec![[0.0, 0.0, 1.0]],
        vec![0, 0, 0],
        vec![[-3.0, 1.0, 2.0]],
        vec![[1.0, 4.0, 5.0]],
    );
    assert_eq!(mesh.bounds.min, Vec3::new(-3.0, 0.0, 0.0));
    assert_eq!(mesh.bounds.max, Vec3::new(1.0, 4.0, 5.0));
    assert_eq!(mesh.triangle_count(), 1);
}

#[test]
fn removing_top_level_nodes_prunes_nested_selection() {
    let mut scene = scene();
    scene.select_only("a1");
    scene.toggle_selected("b");
    assert_eq!(scene.remove_top_level(&["a1".into()]), Vec::<String>::new());
    assert_eq!(scene.remove_top_level(&["a".into()]), ["a"]);
    assert_eq!(scene.selection, ["b"]);
    assert_eq!(scene.top_level_keys(), ["b"]);
}

#[test]
fn clearing_scene_returns_keys_and_removes_selection() {
    let mut scene = scene();
    scene.select_only("a2");
    assert_eq!(scene.clear(), ["a", "b"]);
    assert!(scene.is_empty());
    assert!(scene.selection.is_empty());
    assert!(scene.current().is_none());
}

#[test]
fn visibility_can_be_restored_for_an_entire_subtree() {
    let mut scene = scene();
    scene.set_visible("a", false);
    scene.set_visible("a", true);
    assert_eq!(
        scene
            .visible_nodes()
            .iter()
            .map(|n| n.key.as_str())
            .collect::<Vec<_>>(),
        ["a", "a1", "a2", "b"]
    );
}

#[test]
fn selection_bounds_include_children_and_overlay_affects_visible_bounds() {
    let mut scene = scene();
    scene.select_only("a");
    let selected = scene.selection_bounds();
    assert_eq!(selected.max, Vec3::new(1.0, 2.0, 3.0));
    scene.overlay.meshes.push(Arc::new(Mesh::lines(vec![
        [20.0, 0.0, 0.0],
        [21.0, 0.0, 0.0],
    ])));
    assert_eq!(scene.visible_bounds().max.x, 21.0);
}

#[test]
fn hidden_nodes_are_excluded_from_visible_bounds() {
    let mut scene = scene();
    scene.set_visible("a", false);
    scene.set_visible("b", false);
    assert!(scene.visible_bounds().is_empty());
    scene.select_only("a");
    assert!(!scene.selection_bounds().is_empty());
}

#[test]
fn topology_selection_builds_and_clears_an_exact_highlight() {
    let mut mesh = Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0.0, 0.0, 1.0]; 3],
        vec![0, 1, 2],
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        Vec::new(),
    );
    mesh.topology.face_positions = mesh.positions.clone();
    mesh.topology.face_indices = mesh.indices.clone();
    mesh.topology.triangle_faces = vec![4];
    mesh.topology.segment_edges = vec![7];
    mesh.topology.vertices = vec![[0.0, 0.0, 0.0]];

    let mut part_node = node("n", "part", vec![]);
    part_node.parts[0].mesh = Arc::new(mesh);
    let mut scene = Scene::default();
    scene.append(vec![part_node], None);
    scene.select_only("n");

    let selection = TopologySelection {
        node: "n".into(),
        part: 0,
        entity: TopologyEntity::Face(4),
        position: Vec3::new(0.25, 0.25, 0.0),
    };
    assert!(scene.select_topology(selection.clone()));
    assert_eq!(scene.topology_selection, Some(selection));
    assert_eq!(scene.topology_highlight().unwrap().triangle_count(), 1);

    scene.select_only("n");
    assert!(scene.topology_selection.is_none());
    assert!(scene.topology_highlight().is_none());
}
