import cadquery as cq
import numpy as np
import pytest

from conftest import decode
from cq_worker.tessellation import Tolerance, tessellate


def test_box_mesh():
    mesh = tessellate(cq.Workplane().box(2, 4, 6).val().wrapped, Tolerance())

    assert len(mesh.indices) == 12 * 3  # two triangles per face
    assert mesh.indices.max() < len(mesh.positions)
    assert len(mesh.edges) == 12  # straight edges need one segment each
    assert len(mesh.points) == 0
    assert sorted(set(mesh.triangle_faces)) == [1, 2, 3, 4, 5, 6]
    assert sorted(set(mesh.segment_edges)) == list(range(1, 13))
    assert len(mesh.topology_vertices) == 8
    assert len(mesh.triangle_faces) == len(mesh.face_indices) // 3
    assert len(mesh.segment_edges) == len(mesh.edges)
    lo, hi = mesh.bounds()
    assert lo == pytest.approx([-1, -2, -3])
    assert hi == pytest.approx([1, 2, 3])


def test_normals_are_unit_and_point_outwards():
    mesh = tessellate(cq.Workplane().sphere(5).val().wrapped, Tolerance())

    lengths = np.linalg.norm(mesh.normals, axis=1)
    assert lengths == pytest.approx(np.ones_like(lengths), abs=1e-4)
    # On a sphere centered at the origin the outward normal is the position.
    radial = mesh.positions / np.linalg.norm(mesh.positions, axis=1, keepdims=True)
    assert np.einsum("ij,ij->i", radial, mesh.normals).min() > 0.99


def test_transformed_shapes_are_meshed_in_world_space():
    shape = cq.Workplane().box(10, 20, 30).val()
    moved = shape.moved(cq.Location(cq.Vector(100, 0, 0), cq.Vector(1, 0, 0), 90))
    mesh = tessellate(moved.wrapped, Tolerance())

    lo, hi = mesh.bounds()
    assert lo == pytest.approx([95, -15, -10], abs=1e-4)
    assert hi == pytest.approx([105, 15, 10], abs=1e-4)
    assert mesh.face_positions.min(axis=0) == pytest.approx(lo, abs=1e-4)
    assert mesh.face_positions.max(axis=0) == pytest.approx(hi, abs=1e-4)
    assert mesh.topology_vertices.min(axis=0) == pytest.approx(lo, abs=1e-4)
    assert mesh.topology_vertices.max(axis=0) == pytest.approx(hi, abs=1e-4)


def test_wires_and_vertices():
    wire = tessellate(cq.Workplane().circle(5).val().wrapped, Tolerance())
    assert len(wire.indices) == 0 and len(wire.edges) > 8
    assert set(wire.segment_edges) == {1}

    vertex = tessellate(cq.Vertex.makeVertex(1, 2, 3).wrapped, Tolerance())
    assert vertex.points.tolist() == [[1, 2, 3]]
    assert vertex.topology_vertices.tolist() == [[1, 2, 3]]


def test_empty_shapes_have_no_mesh():
    assert tessellate(cq.Compound.makeCompound([]).wrapped, Tolerance()) is None


def test_message_round_trip():
    mesh = tessellate(cq.Workplane().box(1, 1, 1).val().wrapped, Tolerance())
    message = mesh.to_message()

    assert decode(message["positions"], "<f4", 3).shape == mesh.positions.shape
    assert decode(message["indices"], "<u4", 3).shape == (12, 3)
    assert decode(message["edges"], "<f4", 6).shape == (12, 6)
    topology = message["topology"]
    assert decode(topology["face_indices"], "<u4", 3).shape == (12, 3)
    assert decode(topology["triangle_faces"], "<u4", 1).shape == (12, 1)
    assert decode(topology["segment_edges"], "<u4", 1).shape == (12, 1)
    assert decode(topology["vertices"], "<f4", 3).shape == (8, 3)
    assert message["bbox"] == [[-0.5] * 3, [0.5] * 3]


def test_finer_tolerance_gives_more_triangles():
    shape = cq.Workplane().sphere(5).val().wrapped
    coarse = tessellate(shape, Tolerance(linear=1e-2, angular=0.5))
    fine = tessellate(
        cq.Workplane().sphere(5).val().wrapped, Tolerance(linear=1e-4, angular=0.1)
    )
    assert len(fine.indices) > len(coarse.indices)
