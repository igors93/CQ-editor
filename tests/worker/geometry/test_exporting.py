"""Exports put shapes where the view shows them."""

import cadquery as cq
import pytest

from cq_worker import exporting
from cq_worker.scene import Registry, SceneBuilder, ShownObject
from cq_worker.tessellation import Tolerance


def nested_assembly():
    """A box 50 mm along x in ``sub``, itself at x = 1000 in a red root at
    y = 500: in the world the box spans x 1045…1055, y 495…505."""

    child = cq.Assembly(
        cq.Workplane().box(10, 10, 10), name="child", loc=cq.Location((50, 0, 0))
    )
    sub = cq.Assembly(name="sub", loc=cq.Location((1000, 0, 0)))
    sub.add(child)
    root = cq.Assembly(
        name="root", loc=cq.Location((0, 500, 0)), color=cq.Color(1, 0, 0)
    )
    root.add(sub)
    return root


def build(obj):
    registry = Registry()
    nodes = SceneBuilder(registry, Tolerance()).build({"assy": ShownObject(obj)})
    return registry, nodes


def bounds(shape):
    box = shape.BoundingBox()
    return [round(value, 3) for value in (box.xmin, box.xmax, box.ymin, box.ymax)]


@pytest.mark.parametrize("level", ["root", "sub", "child"])
def test_any_assembly_node_is_exported_where_the_view_shows_it(tmp_path, level):
    registry, nodes = build(nested_assembly())
    node = nodes[0]
    for _ in range(["root", "sub", "child"].index(level)):
        node = node["children"][0]
    key = node["key"]

    step, brep = tmp_path / "part.step", tmp_path / "part.brep"
    exporting.export(registry, [key], "step", str(step))
    exporting.export(registry, [key], "brep", str(brep))

    world = [1045.0, 1055.0, 495.0, 505.0]
    assert bounds(cq.Shape.importBrep(str(brep))) == world
    assert bounds(cq.importers.importStep(str(step)).val()) == world


def step_part_colors(path):
    """The colors of the parts (nodes with a shape) of a STEP file."""

    return [
        node.color.toTuple()[:3] if node.color else None
        for _, node in cq.Assembly.importStep(str(path)).traverse()
        if node.obj is not None
    ]


@pytest.mark.parametrize("level", ["root", "sub"])
def test_parts_are_exported_in_the_color_they_inherit(tmp_path, level):
    # The view draws the box red, as its root; so must the file.
    root = nested_assembly()
    registry, nodes = build(root)
    node = nodes[0] if level == "root" else nodes[0]["children"][0]
    step = tmp_path / "part.step"
    exporting.export(registry, [node["key"]], "step", str(step))

    colors = step_part_colors(step)
    assert colors and all(
        color == pytest.approx((1, 0, 0), abs=1e-3) for color in colors
    ), colors
    # The script's own assembly is not touched.
    assert root.children[0].color is None
    assert root.children[0].children[0].color is None
