"""Interference checks use the placed solids, never their display meshes."""

import cadquery as cq
import pytest
from cq_worker.interference import analyze_interferences
from cq_worker.scene import Registry, SceneBuilder, ShownObject
from cq_worker.tessellation import Tolerance


def assembly_report(hole_diameter, pin_diameter, *, merged=False, pin_x=0):
    plate = cq.Workplane().box(20, 20, 5).faces(">Z").workplane().hole(hole_diameter)
    pin = cq.Workplane().circle(pin_diameter / 2).extrude(10).translate((0, 0, -5))
    assembly = cq.Assembly(name="joint")
    assembly.add(plate, name="plate")
    assembly.add(pin, name="pin", loc=cq.Location((pin_x, 0, 0)))
    registry = Registry()
    (node,) = SceneBuilder(registry, Tolerance(), merge_assemblies=merged).build(
        {"joint": ShownObject(assembly)}
    )
    return analyze_interferences(registry, [node["key"]])


@pytest.mark.parametrize("merged", [False, True])
def test_oversize_pin_interferes_with_hole_in_both_display_modes(merged):
    (report,) = assembly_report(5, 6, merged=merged)
    assert report["kind"] == "interference"
    assert report["overlap_volume"] == pytest.approx(43.1968989869)
    assert {
        report["first"]["name"].split("/")[-1],
        report["second"]["name"].split("/")[-1],
    } == {
        "plate",
        "pin",
    }


def test_smaller_pin_passes_through_exact_hole_without_interference():
    assert assembly_report(6.5, 6) == []


def test_misaligned_pin_interferes_even_when_its_diameter_is_smaller():
    reports = assembly_report(6.5, 6, pin_x=0.5)
    assert len(reports) == 1 and reports[0]["kind"] == "interference"


def test_face_contact_is_separate_from_penetration():
    first = cq.Workplane().box(2, 2, 2)
    second = cq.Workplane().box(2, 2, 2).translate((2, 0, 0))
    registry = Registry()
    nodes = SceneBuilder(registry, Tolerance()).build(
        {"first": ShownObject(first), "second": ShownObject(second)}
    )
    (report,) = analyze_interferences(registry, [node["key"] for node in nodes])
    assert report["kind"] == "contact"
    assert report["overlap_volume"] == pytest.approx(0)


def test_two_instances_of_same_shape_are_not_deduplicated_inside_assembly():
    cube = cq.Workplane().box(2, 2, 2)
    assembly = cq.Assembly().add(cube, name="one").add(cube, name="two")
    registry = Registry()
    (node,) = SceneBuilder(registry, Tolerance()).build(
        {"assembly": ShownObject(assembly)}
    )
    (report,) = analyze_interferences(registry, [node["key"]])
    assert report["kind"] == "interference"
    assert report["overlap_volume"] == pytest.approx(8)
