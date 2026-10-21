"""Triangulate OpenCascade shapes into compact, GUI-ready arrays.

Faces are meshed in parallel by BRepMesh and written out through OCCT's glTF
writer, which emits positions, exact surface normals and indices from C++ in a
single pass. Reading that binary back with numpy is orders of magnitude faster
than walking ``Poly_Triangulation`` nodes from Python. Edges and free vertices
are discretized directly, since glTF has no notion of them.

Every array travels to the GUI as base64-encoded little-endian binary.
"""

from __future__ import annotations

import base64
import json
import os
import struct
import tempfile
from dataclasses import dataclass

import numpy as np
from OCP.BRep import BRep_Tool
from OCP.BRepAdaptor import BRepAdaptor_Curve
from OCP.BRepBndLib import BRepBndLib
from OCP.BRepMesh import BRepMesh_IncrementalMesh
from OCP.Bnd import Bnd_Box
from OCP.GCPnts import GCPnts_TangentialDeflection
from OCP.Message import Message_ProgressRange
from OCP.RWGltf import RWGltf_CafWriter
from OCP.TCollection import TCollection_AsciiString, TCollection_ExtendedString
from OCP.TColStd import TColStd_IndexedDataMapOfStringString
from OCP.TDocStd import TDocStd_Document
from OCP.TopAbs import TopAbs_EDGE, TopAbs_FACE, TopAbs_VERTEX
from OCP.TopExp import TopExp, TopExp_Explorer
from OCP.TopTools import TopTools_IndexedMapOfShape
from OCP.TopoDS import TopoDS, TopoDS_Shape
from OCP.XCAFDoc import XCAFDoc_DocumentTool

# Never mesh finer than this, whatever the model size (model units).
MIN_DEFLECTION = 1e-4

_GLB_MAGIC = 0x46546C67  # "glTF"
_CHUNK_JSON = 0x4E4F534A
_CHUNK_BIN = 0x004E4942
_COMPONENT_TYPES = {
    5121: np.uint8,
    5123: np.uint16,
    5125: np.uint32,
    5126: np.float32,
}
_COMPONENT_COUNTS = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}


class TessellationError(RuntimeError):
    pass


@dataclass(frozen=True)
class Tolerance:
    """Mesh quality settings."""

    #: Chordal deviation, relative to the shape's bounding-box diagonal.
    linear: float = 1e-3
    #: Maximum angle between adjacent mesh normals, in radians.
    angular: float = 0.2


@dataclass
class Mesh:
    positions: np.ndarray  # (n, 3) float32
    normals: np.ndarray  # (n, 3) float32
    indices: np.ndarray  # (m,) uint32, three per triangle
    edges: np.ndarray  # (k, 6) float32, one line segment per row
    points: np.ndarray  # (p, 3) float32

    def is_empty(self) -> bool:
        return not (len(self.indices) or len(self.edges) or len(self.points))

    def bounds(self) -> tuple[list[float], list[float]]:
        coords = [
            a.reshape(-1, 3)
            for a in (self.positions, self.edges, self.points)
            if a.size
        ]
        stacked = np.concatenate(coords)
        return stacked.min(axis=0).tolist(), stacked.max(axis=0).tolist()

    def to_message(self) -> dict:
        lo, hi = self.bounds()
        return {
            "positions": _encode(self.positions, "<f4"),
            "normals": _encode(self.normals, "<f4"),
            "indices": _encode(self.indices, "<u4"),
            "edges": _encode(self.edges, "<f4"),
            "points": _encode(self.points, "<f4"),
            "bbox": [lo, hi],
        }


def tessellate(shape: TopoDS_Shape, tolerance: Tolerance) -> Mesh | None:
    """Mesh ``shape``; returns ``None`` when it has no drawable geometry."""

    box = Bnd_Box()
    BRepBndLib.Add_s(shape, box, False)
    if box.IsVoid():
        return None

    diagonal = box.CornerMin().Distance(box.CornerMax())
    deflection = max(tolerance.linear * diagonal, MIN_DEFLECTION)

    positions, normals, indices = _mesh_faces(shape, deflection, tolerance.angular)
    mesh = Mesh(
        positions=positions,
        normals=normals,
        indices=indices,
        edges=_discretize_edges(shape, deflection, tolerance.angular),
        points=_free_vertices(shape),
    )
    return None if mesh.is_empty() else mesh


def _encode(array: np.ndarray, dtype: str) -> str:
    data = np.ascontiguousarray(array, dtype=dtype).tobytes()
    return base64.b64encode(data).decode("ascii")


def _empty(columns: int) -> np.ndarray:
    return np.zeros((0, columns), dtype=np.float32)


def _mesh_faces(
    shape: TopoDS_Shape, deflection: float, angular: float
) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    if not TopExp_Explorer(shape, TopAbs_FACE).More():
        return _empty(3), _empty(3), np.zeros(0, dtype=np.uint32)

    BRepMesh_IncrementalMesh(shape, deflection, False, angular, True)

    document = TDocStd_Document(TCollection_ExtendedString("XmlOcaf"))
    XCAFDoc_DocumentTool.ShapeTool_s(document.Main()).AddShape(shape, False)

    fd, path = tempfile.mkstemp(prefix="cq-editor-", suffix=".glb")
    os.close(fd)
    try:
        writer = RWGltf_CafWriter(TCollection_AsciiString(path), True)
        writer.SetMergeFaces(True)
        ok = writer.Perform(
            document, TColStd_IndexedDataMapOfStringString(), Message_ProgressRange()
        )
        if not ok:
            raise TessellationError("the glTF writer failed to export the mesh")
        with open(path, "rb") as file:
            data = file.read()
    finally:
        os.unlink(path)

    return read_glb(data)


def read_glb(data: bytes) -> tuple[np.ndarray, np.ndarray, np.ndarray]:
    """Collect every triangle primitive of a binary glTF into world space."""

    magic, version, _length = struct.unpack_from("<III", data, 0)
    if magic != _GLB_MAGIC or version != 2:
        raise TessellationError("unexpected glTF header")

    document, binary = None, b""
    offset = 12
    while offset < len(data):
        chunk_length, chunk_type = struct.unpack_from("<II", data, offset)
        chunk = data[offset + 8 : offset + 8 + chunk_length]
        if chunk_type == _CHUNK_JSON:
            document = json.loads(chunk)
        elif chunk_type == _CHUNK_BIN:
            binary = chunk
        offset += 8 + chunk_length
    if document is None:
        raise TessellationError("glTF file without a JSON chunk")

    reader = _AccessorReader(document, binary)
    positions, normals, indices = [], [], []
    vertex_count = 0

    def visit(node_index: int, parent: np.ndarray) -> None:
        nonlocal vertex_count
        node = document["nodes"][node_index]
        transform = parent @ _node_matrix(node)
        if "mesh" in node:
            rotation = transform[:3, :3]
            for primitive in document["meshes"][node["mesh"]]["primitives"]:
                if primitive.get("mode", 4) != 4:  # only triangle lists
                    continue
                attributes = primitive["attributes"]
                pos = reader.read(attributes["POSITION"]).reshape(-1, 3)
                if "NORMAL" in attributes:
                    nor = reader.read(attributes["NORMAL"]).reshape(-1, 3)
                else:
                    nor = np.zeros_like(pos)
                if "indices" in primitive:
                    idx = reader.read(primitive["indices"]).astype(np.uint32)
                else:
                    idx = np.arange(len(pos), dtype=np.uint32)

                positions.append(pos @ rotation.T + transform[:3, 3])
                nor = nor @ rotation.T
                lengths = np.linalg.norm(nor, axis=1, keepdims=True)
                normals.append(nor / np.where(lengths > 0, lengths, 1))
                indices.append(idx + vertex_count)
                vertex_count += len(pos)
        for child in node.get("children", []):
            visit(child, transform)

    scene = document.get("scenes", [{}])[document.get("scene", 0)]
    for root in scene.get("nodes", []):
        visit(root, np.eye(4))

    if not positions:
        return _empty(3), _empty(3), np.zeros(0, dtype=np.uint32)
    return (
        np.concatenate(positions).astype(np.float32),
        np.concatenate(normals).astype(np.float32),
        np.concatenate(indices),
    )


class _AccessorReader:
    def __init__(self, document: dict, binary: bytes) -> None:
        self._accessors = document.get("accessors", [])
        self._views = document.get("bufferViews", [])
        self._binary = binary

    def read(self, index: int) -> np.ndarray:
        accessor = self._accessors[index]
        view = self._views[accessor["bufferView"]]
        dtype = np.dtype(_COMPONENT_TYPES[accessor["componentType"]]).newbyteorder("<")
        components = _COMPONENT_COUNTS[accessor["type"]]
        count = accessor["count"]
        start = view.get("byteOffset", 0) + accessor.get("byteOffset", 0)
        stride = view.get("byteStride", 0)
        element_size = dtype.itemsize * components

        if stride and stride != element_size:
            rows = np.frombuffer(
                self._binary,
                dtype=np.uint8,
                count=stride * (count - 1) + element_size,
                offset=start,
            )
            rows = np.lib.stride_tricks.as_strided(
                rows, shape=(count, element_size), strides=(stride, 1)
            )
            return np.ascontiguousarray(rows).view(dtype).reshape(-1)
        return np.frombuffer(
            self._binary, dtype=dtype, count=count * components, offset=start
        )


def _node_matrix(node: dict) -> np.ndarray:
    if "matrix" in node:
        # glTF matrices are column-major.
        return np.array(node["matrix"], dtype=float).reshape(4, 4).T

    matrix = np.eye(4)
    if "scale" in node:
        matrix = np.diag([*node["scale"], 1.0]) @ matrix
    if "rotation" in node:
        x, y, z, w = node["rotation"]
        rotation = np.array(
            [
                [1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)],
            ]
        )
        rotated = np.eye(4)
        rotated[:3, :3] = rotation
        matrix = rotated @ matrix
    if "translation" in node:
        translated = np.eye(4)
        translated[:3, 3] = node["translation"]
        matrix = translated @ matrix
    return matrix


def _discretize_edges(
    shape: TopoDS_Shape, deflection: float, angular: float
) -> np.ndarray:
    edge_map = TopTools_IndexedMapOfShape()
    TopExp.MapShapes_s(shape, TopAbs_EDGE, edge_map)

    segments: list[np.ndarray] = []
    for i in range(1, edge_map.Extent() + 1):
        edge = TopoDS.Edge_s(edge_map.FindKey(i))
        if BRep_Tool.Degenerated_s(edge):
            continue
        try:
            sampler = GCPnts_TangentialDeflection(
                BRepAdaptor_Curve(edge), angular, deflection
            )
        except Exception:  # an edge without a 3D curve; nothing to draw
            continue
        count = sampler.NbPoints()
        if count < 2:
            continue
        polyline = np.array(
            [
                (p.X(), p.Y(), p.Z())
                for p in (sampler.Value(j) for j in range(1, count + 1))
            ],
            dtype=np.float32,
        )
        segments.append(np.hstack((polyline[:-1], polyline[1:])))

    return np.concatenate(segments) if segments else _empty(6)


def _free_vertices(shape: TopoDS_Shape) -> np.ndarray:
    """Vertices that are not part of any edge (e.g. a ``.vertices()`` result)."""

    points = []
    explorer = TopExp_Explorer(shape, TopAbs_VERTEX, TopAbs_EDGE)
    while explorer.More():
        p = BRep_Tool.Pnt_s(TopoDS.Vertex_s(explorer.Current()))
        points.append((p.X(), p.Y(), p.Z()))
        explorer.Next()
    return np.array(points, dtype=np.float32).reshape(-1, 3)
