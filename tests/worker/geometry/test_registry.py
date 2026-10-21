"""Scene key lifecycle and export selection."""

import cadquery as cq
import pytest

from cq_worker.scene import Registry, RegistryEntry


def test_subtree_shapes_deduplicates_overlapping_selection():
    registry = Registry()
    first = cq.Workplane().box(1, 1, 1).val()
    second = cq.Workplane().sphere(1).val()
    child = registry.add(RegistryEntry(second, second))
    parent = registry.add(RegistryEntry(first, first, [child]))

    assert registry.subtree_shapes([parent, child, parent]) == [first, second]


def test_release_removes_children_but_keeps_other_roots():
    registry = Registry()
    child = registry.add(RegistryEntry("child", None))
    parent = registry.add(RegistryEntry("parent", None, [child]))
    other = registry.add(RegistryEntry("other", None))
    registry.release([parent, "unknown"])

    assert registry.get(other).obj == "other"
    for key in (parent, child):
        with pytest.raises(KeyError, match="may have been cleared"):
            registry.get(key)


def test_clear_drops_entries_without_reusing_old_keys():
    registry = Registry()
    old = registry.add(RegistryEntry("old", None))
    registry.clear()
    new = registry.add(RegistryEntry("new", None))
    assert old != new
    with pytest.raises(KeyError):
        registry.get(old)


def test_subtree_shapes_skips_nodes_without_geometry():
    registry = Registry()
    shape = cq.Workplane().box(1, 1, 1).val()
    child = registry.add(RegistryEntry(shape, shape))
    parent = registry.add(RegistryEntry("group", None, [child]))
    assert registry.subtree_shapes([parent]) == [shape]
