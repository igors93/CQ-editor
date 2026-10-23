import math

import cadquery as cq
import pytest

from cq_worker.measurement import MeasurementError, measure
from cq_worker.scene import Registry, RegistryEntry
from cq_worker.tessellation import TopologyIndex


def add_shape(registry, shape):
    topology = TopologyIndex.build(shape.wrapped)
    return registry.add(RegistryEntry(shape, shape, topologies=[topology]))


def reference(key, kind, identifier=1):
    return {"key": key, "part": 0, "kind": kind, "identifier": identifier}


def test_two_vertices_have_an_exact_five_unit_distance():
    registry = Registry()
    first = add_shape(registry, cq.Vertex.makeVertex(0, 0, 0))
    second = add_shape(registry, cq.Vertex.makeVertex(3, 4, 0))

    result = measure(
        registry, [reference(first, "vertex"), reference(second, "vertex")]
    )

    assert result["pair"]["distance"] == pytest.approx(5.0, abs=1e-12)
    assert result["pair"]["first_point"] == pytest.approx([0.0, 0.0, 0.0])
    assert result["pair"]["second_point"] == pytest.approx([3.0, 4.0, 0.0])
    assert result["identity"] == {"scope": "scene_entry", "persistent": False}


def test_curved_edge_reports_occt_length_radius_and_diameter():
    registry = Registry()
    key = add_shape(registry, cq.Edge.makeCircle(2.5))

    (entity,) = measure(registry, [reference(key, "edge")])["entities"]

    assert entity["geometry_type"] == "circle"
    assert entity["length"] == pytest.approx(5.0 * math.pi)
    assert entity["radius"] == pytest.approx(2.5)
    assert entity["diameter"] == pytest.approx(5.0)


def test_circular_planar_face_reports_area_and_boundary_radius():
    wire = cq.Wire.makeCircle(3, cq.Vector(), cq.Vector(0, 0, 1))
    face = cq.Face.makeFromWires(wire)
    registry = Registry()
    key = add_shape(registry, face)

    (entity,) = measure(registry, [reference(key, "face")])["entities"]

    assert entity["geometry_type"] == "plane"
    assert entity["area"] == pytest.approx(9.0 * math.pi)
    assert entity["radius"] == pytest.approx(3.0)
    assert entity["diameter"] == pytest.approx(6.0)


def test_cylindrical_face_reports_surface_type_and_radius():
    face = cq.Workplane("XY").cylinder(5, 2).faces("%CYLINDER").val()
    registry = Registry()
    key = add_shape(registry, face)

    (entity,) = measure(registry, [reference(key, "face")])["entities"]

    assert entity["geometry_type"] == "cylinder"
    assert entity["radius"] == pytest.approx(2.0)
    assert entity["diameter"] == pytest.approx(4.0)


def test_angles_are_measured_for_straight_edges_and_planar_faces():
    registry = Registry()
    horizontal = add_shape(registry, cq.Edge.makeLine(cq.Vector(), cq.Vector(4, 0, 0)))
    vertical = add_shape(registry, cq.Edge.makeLine(cq.Vector(), cq.Vector(0, 4, 0)))
    edge_angle = measure(
        registry,
        [reference(horizontal, "edge"), reference(vertical, "edge")],
    )["pair"]

    assert edge_angle["angle_kind"] == "edge_edge"
    assert edge_angle["angle_degrees"] == pytest.approx(90.0)


def test_planar_face_angle_and_edge_to_plane_angle():
    registry = Registry()
    xy_face = cq.Workplane("XY").rect(2, 2).extrude(0.1).faces(">Z").val()
    yz_face = cq.Workplane("YZ").rect(2, 2).extrude(0.1).faces(">X").val()
    edge = cq.Edge.makeLine(cq.Vector(), cq.Vector(0, 0, 4))
    xy_key = add_shape(registry, xy_face)
    yz_key = add_shape(registry, yz_face)
    edge_key = add_shape(registry, edge)

    faces = measure(registry, [reference(xy_key, "face"), reference(yz_key, "face")])[
        "pair"
    ]
    edge_face = measure(
        registry, [reference(edge_key, "edge"), reference(xy_key, "face")]
    )["pair"]

    assert faces["angle_kind"] == "face_face"
    assert faces["angle_degrees"] == pytest.approx(90.0)
    assert edge_face["angle_kind"] == "edge_face"
    assert edge_face["angle_degrees"] == pytest.approx(90.0)


def test_rejects_empty_or_oversized_selections_and_stale_references():
    registry = Registry()
    key = add_shape(registry, cq.Vertex.makeVertex(0, 0, 0))
    ref = reference(key, "vertex")

    with pytest.raises(MeasurementError, match="between one and 64"):
        measure(registry, [])
    with pytest.raises(MeasurementError, match="between one and 64"):
        measure(registry, [ref] * 65)
    registry.release([key])
    with pytest.raises(MeasurementError, match="previous Python session"):
        measure(registry, [ref])


def test_more_than_two_entities_are_described_without_a_pair():
    registry = Registry()
    box = cq.Workplane().box(1, 2, 3).val()
    key = add_shape(registry, box)

    result = measure(registry, [reference(key, "face", i) for i in (1, 2, 3)])

    assert [entity["identifier"] for entity in result["entities"]] == [1, 2, 3]
    assert all(entity["area"] > 0 for entity in result["entities"])
    assert result["pair"] is None
