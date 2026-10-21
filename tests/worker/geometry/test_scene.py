import cadquery as cq
import pytest

from cq_worker.scene import (
    DisplayOptions,
    Registry,
    SceneBuilder,
    ShownObject,
    find_cq_objects,
    parse_color,
    to_compound,
)
from cq_worker.tessellation import GeometryCache, Tolerance


def test_to_compound_applies_sketch_placement():
    sketch = cq.Sketch().rect(1, 1)
    placed = cq.Workplane("XZ").placeSketch(sketch).val()
    (face,) = to_compound(placed).Faces()

    assert face.Area() == pytest.approx(1)
    assert face.normalAt().toTuple() == pytest.approx((0, -1, 0))


def test_to_compound_empty_list():
    # #600: an empty list must give an empty compound instead of raising.
    result = to_compound([])

    assert isinstance(result, cq.Compound)
    assert result.Solids() == []


def test_to_compound_skips_non_shape_stack_entries():
    points_only = cq.Workplane().pushPoints([(0, 0), (1, 1)])
    assert to_compound(points_only).Vertices() == []


def test_to_compound_accepts_mixed_lists():
    box = cq.Workplane().box(1, 1, 1)
    sphere = cq.Workplane().sphere(1).val()
    assert len(to_compound([box, sphere]).Solids()) == 2


def test_to_compound_rejects_unknown_types():
    with pytest.raises(ValueError):
        to_compound(42)


@pytest.mark.parametrize(
    "spec, expected",
    [
        ("#ff0000", ((1.0, 0.0, 0.0), None)),
        ("#0f08", ((0.0, 1.0, 0.0), 0x88 / 255)),
        ((255, 0, 0), ((1.0, 0.0, 0.0), None)),
        ((0.5, 0.25, 1.0, 0.5), ((0.5, 0.25, 1.0), 0.5)),
    ],
)
def test_parse_color(spec, expected):
    rgb, alpha = parse_color(spec)
    assert rgb == pytest.approx(expected[0])
    assert alpha == (pytest.approx(expected[1]) if expected[1] is not None else None)


def test_parse_color_names_and_cq_colors_are_srgb():
    rgb, alpha = parse_color("lightgray")
    assert rgb == pytest.approx((211 / 255,) * 3, abs=1e-3)
    assert parse_color(cq.Color("lightgray"))[0] == pytest.approx(rgb)


def test_parse_color_rejects_garbage():
    with pytest.raises(ValueError):
        parse_color((300, 0, 0))
    with pytest.raises(ValueError):
        parse_color(object())


def test_display_options():
    options = DisplayOptions.parse(
        {"color": "red", "alpha": 0.5, "edgecolor": (0, 0, 255)}
    )
    assert options.color == pytest.approx((1, 0, 0))
    assert options.alpha == 0.5
    assert options.edge_color == pytest.approx((0, 0, 1))

    rgba = DisplayOptions.parse({"rgba": (0.1, 0.2, 0.3, 0.4)})
    assert rgba.color == pytest.approx((0.1, 0.2, 0.3)) and rgba.alpha == 0.4

    # Invalid entries are dropped instead of failing the whole run.
    assert DisplayOptions.parse({"color": "no-such-color"}).color is None


def test_find_cq_objects_ignores_private_and_plain_values():
    found = find_cq_objects(
        {"box": cq.Workplane().box(1, 1, 1), "_hidden": cq.Workplane(), "n": 3}
    )
    assert list(found) == ["box"]


def build(objects, merge=False):
    registry = Registry()
    builder = SceneBuilder(registry, Tolerance(), merge_assemblies=merge)
    nodes = builder.build({name: ShownObject(obj) for name, obj in objects.items()})
    return registry, nodes


def test_builder_creates_one_node_per_object():
    registry, nodes = build(
        {
            "box": cq.Workplane().box(1, 2, 3),
            "empty": cq.Workplane().pushPoints([(0, 0)]),
        }
    )

    assert [n["name"] for n in nodes] == ["box"]
    node = nodes[0]
    assert node["kind"] == "Workplane"
    assert node["inspectable"] is True
    assert len(node["parts"]) == 1
    entry = registry.get(node["key"])
    assert entry.shape is not None
    assert len(entry.topologies) == 1


def make_assembly():
    plate = cq.Workplane().box(10, 10, 1)
    pin = cq.Workplane().cylinder(5, 1)
    sub = cq.Assembly(name="pins").add(pin, name="pin", color=cq.Color("red"))
    return (
        cq.Assembly(name="top")
        .add(plate, name="plate")
        .add(sub, loc=cq.Location(cq.Vector(0, 0, 3)), color=cq.Color("blue"))
    )


def test_builder_explodes_assemblies():
    registry, nodes = build({"assy": make_assembly()})

    (root,) = nodes
    assert [c["name"] for c in root["children"]] == ["plate", "pins"]
    pins = root["children"][1]
    pin = pins["children"][0]
    # The part's own color wins over the inherited one.
    assert pin["parts"][0]["color"] == pytest.approx((1, 0, 0))
    # Geometry stays in local coordinates and locations accumulate in the
    # instance transform, allowing identical parts to share one mesh.
    bbox = pin["parts"][0]["mesh"]["bbox"]
    assert bbox[0][2] == pytest.approx(-2.5, abs=1e-3)
    assert bbox[1][2] == pytest.approx(2.5, abs=1e-3)
    assert pin["parts"][0]["transform"][14] == pytest.approx(3)
    assert len(registry.subtree_shapes([root["key"]])) == 2


def test_builder_can_merge_assemblies():
    registry, nodes = build({"assy": make_assembly()}, merge=True)

    (root,) = nodes
    assert root["children"] == []
    assert len(root["parts"]) == 2
    entry = registry.get(root["key"])
    assert len(entry.topologies) == len(root["parts"])
    assert entry.subshape(1, "face", 1).ShapeType() == "Face"


def test_registry_release_drops_subtrees():
    registry, nodes = build({"assy": make_assembly()})
    root = nodes[0]
    registry.release([root["key"]])
    with pytest.raises(KeyError):
        registry.get(root["children"][0]["key"])


def test_known_geometry_is_sent_as_a_reference_without_tessellating_again(
    monkeypatch,
):
    cache = GeometryCache()
    first_builder = SceneBuilder(Registry(), Tolerance(), geometry_cache=cache)
    first = first_builder.build({"box": ShownObject(cq.Workplane().box(1, 2, 3))})
    geometry_id = first[0]["parts"][0]["mesh"]["id"]

    def unexpected(*_args, **_kwargs):
        raise AssertionError("known geometry was tessellated again")

    monkeypatch.setattr(cache, "tessellate", unexpected)
    second_builder = SceneBuilder(
        Registry(),
        Tolerance(),
        geometry_cache=cache,
        known_geometry=[geometry_id],
    )
    second = second_builder.build({"box": ShownObject(cq.Workplane().box(1, 2, 3))})

    assert second[0]["parts"][0]["mesh"] == {"id": geometry_id}


def test_repeated_assembly_parts_share_local_geometry():
    profile = cq.Workplane().box(40, 20, 1000)
    assembly = (
        cq.Assembly()
        .add(profile, name="left", loc=cq.Location(cq.Vector(0, 0, 0)))
        .add(profile, name="right", loc=cq.Location(cq.Vector(5000, 0, 0)))
    )
    _, nodes = build({"frame": assembly})
    left, right = nodes[0]["children"]
    left_part, right_part = left["parts"][0], right["parts"][0]

    assert left_part["mesh"]["id"] == right_part["mesh"]["id"]
    assert "bbox" in left_part["mesh"]
    assert right_part["mesh"] == {"id": left_part["mesh"]["id"]}
    assert left_part["transform"][12:15] == pytest.approx([0, 0, 0])
    assert right_part["transform"][12:15] == pytest.approx([5000, 0, 0])
