use super::*;

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

#[test]
fn decodes_meshes() {
    let positions: Vec<u8> = [0.0f32, 1.0, 2.0, 3.0, 4.0, 5.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let indices: Vec<u8> = [0u32, 1, 1].iter().flat_map(|v| v.to_le_bytes()).collect();
    let json = format!(
        r#"{{"type":"inspection_geometry","id":3,"plane":null,"meshes":[{{
            "positions":"{p}","normals":"{p}","indices":"{i}","edges":"","points":"",
            "topology":{{"face_positions":"{p}","face_indices":"{i}",
                "triangle_faces":"{f}","segment_edges":"","vertices":""}},
            "bbox":[[0,1,2],[3,4,5]]}}]}}"#,
        p = b64(&positions),
        i = b64(&indices),
        f = b64(&1u32.to_le_bytes()),
    );
    let Message::InspectionGeometry { id, meshes, plane } = serde_json::from_str(&json).unwrap()
    else {
        panic!("wrong message type");
    };
    assert_eq!(id, 3);
    assert!(plane.is_none());
    assert_eq!(meshes[0].positions, vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]]);
    assert_eq!(meshes[0].indices, vec![0, 1, 1]);
    assert!(meshes[0].edges.is_empty());
    assert_eq!(meshes[0].topology.triangle_faces, vec![1]);
}

#[test]
fn serializes_requests_with_command_tags() {
    let request = Request::Debug {
        action: DebugAction::StepIn,
        breakpoints: vec![3],
    };
    assert_eq!(
        serde_json::to_string(&request).unwrap(),
        r#"{"cmd":"debug","action":"step_in","breakpoints":[3]}"#
    );
    assert_eq!(
        serde_json::to_string(&Request::Shutdown).unwrap(),
        r#"{"cmd":"shutdown"}"#
    );
}

#[test]
fn measurement_request_and_exact_response_share_topology_references() {
    let request = Request::Measure {
        id: 12,
        selections: vec![TopologyReference {
            key: "s1-n2".into(),
            part: 0,
            kind: TopologyKind::Vertex,
            identifier: 3,
        }],
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["cmd"], "measure");
    assert_eq!(json["selections"][0]["kind"], "vertex");
    assert_eq!(json["selections"][0]["identifier"], 3);

    let message: Message = serde_json::from_value(serde_json::json!({
        "type": "measurement",
        "id": 12,
        "identity": {"scope": "scene_entry", "persistent": false},
        "entities": [{
            "key": "s1-n2", "part": 0, "kind": "vertex", "identifier": 3,
            "geometry_type": "vertex", "center": [3.0, 4.0, 0.0],
            "position": [3.0, 4.0, 0.0], "length": null, "area": null,
            "radius": null, "diameter": null
        }],
        "pair": null
    }))
    .unwrap();
    let Message::Measurement(measurement) = message else {
        panic!("expected measurement");
    };
    assert_eq!(measurement.entities[0].position, Some([3.0, 4.0, 0.0]));
    assert!(!measurement.identity.persistent);
}

#[test]
fn parses_errors_and_output() {
    let message: Message =
        serde_json::from_str(r#"{"type":"output","id":null,"stream":"stderr","text":"x"}"#)
            .unwrap();
    assert!(matches!(
        message,
        Message::Output {
            id: None,
            stream: OutputStream::Stderr,
            ..
        }
    ));
}

#[test]
fn serializes_run_request_with_worker_settings() {
    let request = Request::Run {
        id: 9,
        script: "show_object(cq.Workplane())".into(),
        path: None,
        debug: false,
        breakpoints: vec![4],
        settings: RunSettings {
            reload_cq: false,
            add_script_dir_to_path: true,
            change_working_dir: true,
            reload_imported_modules: true,
        },
        scene: SceneSettings {
            linear_tolerance: 0.001,
            angular_tolerance: 0.2,
            merge_assemblies: false,
        },
        known_geometry: vec!["abc".into()],
    };
    let json = serde_json::to_value(request).unwrap();
    assert_eq!(json["cmd"], "run");
    assert_eq!(json["id"], 9);
    assert_eq!(json["path"], serde_json::Value::Null);
    assert_eq!(json["breakpoints"], serde_json::json!([4]));
    assert_eq!(json["settings"]["add_script_dir_to_path"], true);
    assert_eq!(json["scene"]["linear_tolerance"], 0.001);
    assert_eq!(json["known_geometry"], serde_json::json!(["abc"]));
}

#[test]
fn parses_worker_readiness_and_optional_fatal_detail() {
    let ready: Message = serde_json::from_value(serde_json::json!({
        "type": "ready", "protocol": PROTOCOL_VERSION,
        "python": "3.12", "cadquery": "2.7", "executable": "python",
        "completion": true, "pid": 123
    }))
    .unwrap();
    let Message::Ready(info) = ready else {
        panic!("expected ready");
    };
    assert_eq!(info.protocol, PROTOCOL_VERSION);
    assert!(info.completion);

    let fatal: Message =
        serde_json::from_str(r#"{"type":"fatal","message":"no cadquery"}"#).unwrap();
    assert!(matches!(fatal, Message::Fatal { detail: None, .. }));
}

#[test]
fn rejects_corrupt_base64_mesh_data() {
    let base = serde_json::json!({
        "positions": "!!!", "normals": "", "indices": "",
        "edges": "", "points": "",
        "topology": {
            "face_positions": "", "face_indices": "", "triangle_faces": "",
            "segment_edges": "", "vertices": ""
        },
        "bbox": [[0,0,0],[1,1,1]]
    });
    assert!(serde_json::from_value::<Mesh>(base.clone()).is_err());

    let mut short_vector = base.clone();
    short_vector["positions"] = b64(&[0; 8]).into();
    assert!(serde_json::from_value::<Mesh>(short_vector).is_err());

    let mut short_index = base;
    short_index["positions"] = "".into();
    short_index["indices"] = b64(&[0; 3]).into();
    assert!(serde_json::from_value::<Mesh>(short_index).is_err());

    let mut short_topology = serde_json::json!({
        "positions": "", "normals": "", "indices": "", "edges": "", "points": "",
        "topology": {
            "face_positions": b64(&[0; 8]), "face_indices": "", "triangle_faces": "",
            "segment_edges": "", "vertices": ""
        },
        "bbox": [[0,0,0],[1,1,1]]
    });
    assert!(serde_json::from_value::<Mesh>(short_topology.clone()).is_err());
    short_topology["topology"]["face_positions"] = "".into();
    short_topology["topology"]["triangle_faces"] = b64(&[0; 3]).into();
    assert!(serde_json::from_value::<Mesh>(short_topology).is_err());
}

#[test]
fn export_formats_have_distinct_extensions_and_wire_values() {
    let expected = [
        (ExportFormat::Step, "step", "STEP"),
        (ExportFormat::Stl, "stl", "STL"),
        (ExportFormat::Brep, "brep", "BREP"),
    ];
    for (format, extension, label) in expected {
        assert_eq!(format.extension(), extension);
        assert_eq!(format.label(), label);
        assert_eq!(
            serde_json::to_string(&format).unwrap(),
            format!("\"{extension}\"")
        );
    }
}

#[test]
fn unknown_worker_message_is_rejected() {
    assert!(serde_json::from_str::<Message>(r#"{"type":"unknown"}"#).is_err());
}
