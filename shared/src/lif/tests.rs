use super::*;
use crate::nurbs::open_uniform_knots;

/// Minimal well-formed two-node layout, as a JSON string so the tests exercise
/// real deserialization rather than hand-built structs.
fn sample_json() -> String {
    r#"{
      "metaInformation": {
        "projectIdentification": "test-project",
        "creator": "rustrack-tests",
        "exportTimestamp": "2026-07-19T00:00:00Z",
        "lifVersion": "1.0.0"
      },
      "layouts": [{
        "layoutId": "L1",
        "layoutName": "Test",
        "nodes": [
          {
            "nodeId": "N1",
            "nodePosition": { "x": 0.0, "y": 0.0 },
            "vehicleTypeNodeProperties": [{ "vehicleTypeId": "vt-a", "theta": 0.0 }]
          },
          {
            "nodeId": "N2",
            "nodePosition": { "x": 10.0, "y": 0.0 },
            "vehicleTypeNodeProperties": [
              { "vehicleTypeId": "vt-a" },
              { "vehicleTypeId": "vt-b" }
            ]
          }
        ],
        "edges": [{
          "edgeId": "E1",
          "startNodeId": "N1",
          "endNodeId": "N2",
          "vehicleTypeEdgeProperties": [{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }]
        }],
        "stations": [{
          "stationId": "S1",
          "interactionNodeIds": ["N2"],
          "stationPosition": { "x": 10.0, "y": 1.0, "theta": 1.57 }
        }]
      }]
    }"#
    .to_string()
}

fn parse(json: &str) -> Lif {
    serde_json::from_str(json).expect("sample should deserialize")
}

#[test]
fn deserializes_and_round_trips() {
    let lif = parse(&sample_json());
    assert_eq!(lif.meta_information.lif_version, "1.0.0");
    assert_eq!(lif.layouts.len(), 1);
    assert_eq!(lif.layouts[0].nodes.len(), 2);
    assert_eq!(lif.layouts[0].stations.len(), 1);

    // Re-serializing and re-parsing must be stable, since the backend serves
    // stored layouts back out.
    let out = serde_json::to_string(&lif).expect("serialize");
    let again: Lif = serde_json::from_str(&out).expect("re-parse");
    assert_eq!(again.layouts[0].edges[0].edge_id, "E1");
}

#[test]
fn applies_spec_defaults() {
    let lif = parse(&sample_json());
    let props = &lif.layouts[0].edges[0].vehicle_type_edge_properties[0];
    // reentryAllowed defaults to true when absent.
    assert!(props.reentry_allowed);
    // Absent optional fields stay absent rather than becoming zero.
    assert!(props.trajectory.is_none());
    assert!(props.max_rotation_speed.is_none());
}

#[test]
fn valid_file_passes() {
    assert!(validate(&parse(&sample_json())).is_ok());
}

#[test]
fn rejects_dangling_edge_endpoint() {
    let json = sample_json().replace(r#""endNodeId": "N2""#, r#""endNodeId": "N99""#);
    let err = validate(&parse(&json)).expect_err("dangling endpoint must fail");
    assert!(matches!(
        err.errors[0],
        LifError::UnknownNodeRef { ref node_id, .. } if node_id == "N99"
    ));
}

#[test]
fn rejects_station_referencing_unknown_node() {
    let json = sample_json().replace(r#""interactionNodeIds": ["N2"]"#, r#""interactionNodeIds": ["N7"]"#);
    let err = validate(&parse(&json)).expect_err("bad station ref must fail");
    assert!(matches!(
        err.errors[0],
        LifError::UnknownNodeRef { ref referenced_by, ref node_id, .. }
            if referenced_by == "S1" && node_id == "N7"
    ));
}

#[test]
fn rejects_duplicate_node_id() {
    let json = sample_json().replace(r#""nodeId": "N2""#, r#""nodeId": "N1""#);
    let err = validate(&parse(&json)).expect_err("duplicate node id must fail");
    assert!(err.errors.iter().any(|e| matches!(
        e,
        LifError::DuplicateId { kind: IdKind::Node, id, .. } if id == "N1"
    )));
}

/// A trajectory whose knot vector is the wrong length would panic inside
/// `find_span` at evaluation time, so it has to be caught up front.
#[test]
fn rejects_malformed_knot_vector() {
    let json = sample_json().replace(
        r#"{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }"#,
        r#"{ "vehicleTypeId": "vt-a", "trajectory": {
            "degree": 2,
            "knotVector": [0.0, 0.0, 1.0],
            "controlPoints": [
              { "x": 0.0, "y": 0.0 }, { "x": 5.0, "y": 2.0 }, { "x": 10.0, "y": 0.0 }
            ]
        }}"#,
    );
    let err = validate(&parse(&json)).expect_err("bad knot vector must fail");
    assert!(matches!(
        err.errors[0],
        LifError::InvalidTrajectory { ref edge_id, .. } if edge_id == "E1"
    ));
}

#[test]
fn rejects_non_decreasing_knot_vector() {
    let json = sample_json().replace(
        r#"{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }"#,
        r#"{ "vehicleTypeId": "vt-a", "trajectory": {
            "degree": 2,
            "knotVector": [0.0, 0.0, 0.0, 1.0, 0.5, 1.0],
            "controlPoints": [
              { "x": 0.0, "y": 0.0 }, { "x": 5.0, "y": 2.0 }, { "x": 10.0, "y": 0.0 }
            ]
        }}"#,
    );
    let err = validate(&parse(&json)).expect_err("out-of-order knots must fail");
    assert!(matches!(err.errors[0], LifError::InvalidTrajectory { .. }));
}

#[test]
fn reports_many_errors_at_once() {
    // Both endpoints dangle and the station reference dangles: three problems,
    // all reported from a single pass.
    let json = sample_json()
        .replace(r#""startNodeId": "N1""#, r#""startNodeId": "X1""#)
        .replace(r#""endNodeId": "N2""#, r#""endNodeId": "X2""#)
        .replace(r#""interactionNodeIds": ["N2"]"#, r#""interactionNodeIds": ["X3"]"#);
    let err = validate(&parse(&json)).expect_err("must fail");
    assert_eq!(err.len(), 3, "should not bail on the first problem");
}

#[test]
fn resolves_to_requested_vehicle_type() {
    let lif = parse(&sample_json());
    let layout = lif.resolve(Some("L1"), "vt-a").expect("vt-a resolves");
    assert_eq!(layout.nodes.len(), 2);
    assert_eq!(layout.edges.len(), 1);
    assert_eq!(layout.edges[0].max_speed, 2.0);
    assert_eq!(layout.nodes[0].theta, Some(0.0));
}

/// N1 has no `vt-b` entry, so it is unavailable to that vehicle — and the edge
/// depending on it must drop out too rather than dangle.
#[test]
fn excludes_nodes_and_dependent_edges_for_other_vehicle_type() {
    let lif = parse(&sample_json());
    let layout = lif.resolve(Some("L1"), "vt-b").expect("vt-b has one node");
    assert_eq!(layout.nodes.len(), 1);
    assert_eq!(layout.nodes[0].node_id, "N2");
    assert_eq!(layout.nodes_excluded, 1);
    assert!(layout.edges.is_empty(), "edge must drop with its endpoint");
    assert_eq!(layout.edges_excluded, 1);
}

#[test]
fn unknown_vehicle_type_is_an_error_not_an_empty_graph() {
    let lif = parse(&sample_json());
    let err = lif.resolve(Some("L1"), "nope").expect_err("must fail");
    assert!(matches!(
        err.errors[0],
        LifError::NoSuchVehicleType { ref vehicle_type_id, .. } if vehicle_type_id == "nope"
    ));
}

#[test]
fn unknown_layout_is_an_error() {
    let lif = parse(&sample_json());
    assert!(lif.resolve(Some("missing"), "vt-a").is_err());
}

/// A single-layout file needs no explicit id; a multi-layout one does.
#[test]
fn layout_id_optional_only_when_unambiguous() {
    let lif = parse(&sample_json());
    assert!(lif.resolve(None, "vt-a").is_ok());

    let two = sample_json().replace(
        r#""layouts": ["#,
        r#""layouts": [{
            "layoutId": "L0", "nodes": [], "edges": [], "stations": []
        },"#,
    );
    let lif2 = parse(&two);
    assert!(lif2.resolve(None, "vt-a").is_err(), "ambiguous must fail");
    assert!(lif2.resolve(Some("L1"), "vt-a").is_ok());
}

/// The simulator previously synthesized knot vectors with `open_uniform_knots`.
/// A LIF file authoring the equivalent vector must produce the identical curve,
/// or converting the existing example silently changes how AGVs move.
#[test]
fn authored_knots_match_previously_synthesized_curve() {
    let knots = open_uniform_knots(3, 2);
    let knot_json = serde_json::to_string(&knots).unwrap();

    let json = sample_json().replace(
        r#"{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }"#,
        &format!(
            r#"{{ "vehicleTypeId": "vt-a", "trajectory": {{
                "degree": 2,
                "knotVector": {knot_json},
                "controlPoints": [
                  {{ "x": 5.0, "y": 0.0 }}, {{ "x": 7.5, "y": 1.5 }}, {{ "x": 10.0, "y": 0.0 }}
                ]
            }}}}"#
        ),
    );
    let lif = parse(&json);
    validate(&lif).expect("authored curve should be valid");

    let layout = lif.resolve(Some("L1"), "vt-a").unwrap();
    let curve = layout.edges[0].curve.as_ref().expect("curve present");
    assert!(curve.is_valid(), "resolved curve must be evaluable");

    // Same control points and same knots as the old synthesized path.
    let expected = crate::nurbs::NurbsCurve {
        degree: 2,
        knots,
        control_points: vec![
            crate::nurbs::ControlPoint { x: 5.0, y: 0.0, weight: 1.0 },
            crate::nurbs::ControlPoint { x: 7.5, y: 1.5, weight: 1.0 },
            crate::nurbs::ControlPoint { x: 10.0, y: 0.0, weight: 1.0 },
        ],
    };
    let a = curve.arc_length_table(50);
    let b = expected.arc_length_table(50);
    assert!(
        (a.last().unwrap().0 - b.last().unwrap().0).abs() < 1e-9,
        "arc length must be unchanged"
    );
}

/// Swap E1's plain properties for a trajectory that bulges 2 m above the
/// straight line between N1 (0,0) and N2 (10,0).
fn json_with_curved_edge() -> String {
    let knots = serde_json::to_string(&open_uniform_knots(3, 2)).unwrap();
    sample_json().replace(
        r#"{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }"#,
        &format!(
            r#"{{ "vehicleTypeId": "vt-a", "trajectory": {{
                "degree": 2,
                "knotVector": {knots},
                "controlPoints": [
                  {{ "x": 0.0, "y": 0.0 }}, {{ "x": 5.0, "y": 4.0 }}, {{ "x": 10.0, "y": 0.0 }}
                ]
            }}}}"#
        ),
    )
}

#[test]
fn map_view_projects_the_whole_layout() {
    let lif = parse(&sample_json());
    let map = lif.map_view(None).expect("single layout projects");

    assert_eq!(map.layout_id, "L1");
    assert_eq!(map.layout_name.as_deref(), Some("Test"));
    assert_eq!(map.available_layouts, vec!["L1".to_string()]);
    assert_eq!(map.nodes.len(), 2);
    assert_eq!(map.edges.len(), 1);
    assert_eq!(map.nodes[0].theta, Some(0.0));
    // N2 constrains no orientation for either vehicle type.
    assert_eq!(map.nodes[1].theta, None);

    // Station carries its own position and name through.
    assert_eq!(map.stations.len(), 1);
    assert_eq!(map.stations[0].id, "S1");
    assert_eq!(map.stations[0].x, 10.0);
    assert_eq!(map.stations[0].y, 1.0);

    // Bounds span the nodes and the station above N2.
    let b = map.bounds.expect("layout has geometry");
    assert_eq!((b.min_x, b.min_y, b.max_x, b.max_y), (0.0, 0.0, 10.0, 1.0));
}

/// The distinction from `resolve`: a map shows the whole track. N1 declares
/// only `vt-a` and E1 only carries `vt-a` properties, yet drawing must not drop
/// either the way `resolve(_, "vt-b")` does.
#[test]
fn map_view_takes_the_union_across_vehicle_types() {
    let lif = parse(&sample_json());
    let map = lif.map_view(None).expect("projects");

    let ids: Vec<&str> = map.nodes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["N1", "N2"]);
    assert_eq!(map.edges[0].id, "E1");
}

#[test]
fn map_view_draws_untrajectoried_edge_as_a_segment() {
    let lif = parse(&sample_json());
    let map = lif.map_view(None).expect("projects");

    // No trajectory authored: exactly the two endpoints, in order.
    assert_eq!(map.edges[0].points, vec![[0.0, 0.0], [10.0, 0.0]]);
}

#[test]
fn map_view_tessellates_a_trajectory() {
    let lif = parse(&json_with_curved_edge());
    validate(&lif).expect("curve should be valid");
    let map = lif.map_view(None).expect("projects");

    let points = &map.edges[0].points;
    assert!(points.len() > 2, "a curve must not collapse to a segment");
    // Endpoints are interpolated by a clamped knot vector, so the polyline
    // still starts and ends on the nodes.
    assert!((points[0][0] - 0.0).abs() < 1e-9 && (points[0][1] - 0.0).abs() < 1e-9);
    let last = points.last().unwrap();
    assert!((last[0] - 10.0).abs() < 1e-9 && (last[1] - 0.0).abs() < 1e-9);

    // The bulge peaks at half the control point's height for a quadratic, and
    // bounds must grow to include it — nodes alone would report maxY = 1.0
    // (the station) and clip the curve.
    let b = map.bounds.expect("has geometry");
    assert!(b.max_y > 1.5, "bounds must cover the curve, got {}", b.max_y);
}

/// A stored document can predate a validation rule, and a map is not the place
/// to start failing: a curve that cannot be evaluated degrades to a straight
/// line rather than panicking inside `find_span`.
#[test]
fn map_view_degrades_malformed_curve_to_a_segment() {
    let json = sample_json().replace(
        r#"{ "vehicleTypeId": "vt-a", "maxSpeed": 2.0 }"#,
        r#"{ "vehicleTypeId": "vt-a", "trajectory": {
            "degree": 2,
            "knotVector": [0.0, 0.0, 1.0],
            "controlPoints": [
              { "x": 0.0, "y": 0.0 }, { "x": 5.0, "y": 2.0 }, { "x": 10.0, "y": 0.0 }
            ]
        }}"#,
    );
    let lif = parse(&json);
    assert!(validate(&lif).is_err(), "fixture should be the rejected one");

    let map = lif.map_view(None).expect("projects anyway");
    assert_eq!(map.edges[0].points, vec![[0.0, 0.0], [10.0, 0.0]]);
}

/// Unlike `resolve`, an ambiguous or unknown layout id is not an error — a
/// viewer should show something rather than nothing.
#[test]
fn map_view_falls_back_to_the_first_layout() {
    let two = sample_json().replace(
        r#""layouts": ["#,
        r#""layouts": [{
            "layoutId": "L0", "nodes": [], "edges": [], "stations": []
        },"#,
    );
    let lif = parse(&two);

    assert!(lif.resolve(None, "vt-a").is_err(), "resolve stays strict");
    assert_eq!(lif.map_view(None).unwrap().layout_id, "L0");
    assert_eq!(lif.map_view(Some("L1")).unwrap().layout_id, "L1");
    assert_eq!(lif.map_view(Some("nope")).unwrap().layout_id, "L0");

    // Both ids offered so a client can build a layer selector from one fetch.
    assert_eq!(
        lif.map_view(None).unwrap().available_layouts,
        vec!["L0".to_string(), "L1".to_string()]
    );
    // An empty layout is drawable, it just has nothing to draw.
    assert!(lif.map_view(Some("L0")).unwrap().bounds.is_none());
}

#[test]
fn map_view_places_positionless_station_at_its_interaction_node() {
    let json = sample_json().replace(
        r#""stationPosition": { "x": 10.0, "y": 1.0, "theta": 1.57 }"#,
        r#""stationName": "Pick A""#,
    );
    let map = parse(&json).map_view(None).expect("projects");

    assert_eq!(map.stations[0].name.as_deref(), Some("Pick A"));
    // Falls back to N2, the node it is interacted with from.
    assert_eq!((map.stations[0].x, map.stations[0].y), (10.0, 0.0));
}

#[test]
fn map_view_serializes_camel_case() {
    let lif = parse(&sample_json());
    let json = serde_json::to_value(lif.map_view(None).unwrap()).unwrap();

    assert!(json.get("layoutId").is_some());
    assert!(json.get("availableLayouts").is_some());
    assert!(json["bounds"].get("minX").is_some());
    // Points stay as bare [x, y] pairs rather than named fields — half the
    // bytes, and the payload is mostly points.
    assert_eq!(json["edges"][0]["points"][0], serde_json::json!([0.0, 0.0]));
}

#[test]
fn summary_counts_across_layouts() {
    let lif = parse(&sample_json());
    let s = LifSummary::derive(&lif, 1234, "2026-07-19T00:00:00Z".to_string());
    assert_eq!(s.project_identification, "test-project");
    assert_eq!(s.layout_count, 1);
    assert_eq!(s.node_count, 2);
    assert_eq!(s.edge_count, 1);
    assert_eq!(s.station_count, 1);
    assert_eq!(s.raw_bytes, 1234);
}
