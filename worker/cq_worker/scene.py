"""Turn the objects a script shows into scene nodes the GUI can draw.

A scene node carries display options, triangulated geometry and, for
assemblies, child nodes mirroring the assembly hierarchy. Every node gets a
key; the :class:`Registry` maps keys back to the live CadQuery objects so the
GUI can later ask for exports or stack inspection by key.
"""

from __future__ import annotations

import itertools
import logging
from collections.abc import Iterable, Mapping
from dataclasses import dataclass, field
from typing import Any

import cadquery as cq
from OCP.TopoDS import TopoDS_Shape

from .tessellation import GeometryCache, Tolerance, TopologyIndex

log = logging.getLogger(__name__)

Color = tuple[float, float, float]


def location_matrix(location: cq.Location) -> list[float]:
    """Return an OCCT rigid transform in column-major order for glam/WebGPU."""

    transform = location.wrapped.Transformation()
    return [
        transform.Value(row, column) if row <= 3 else (1.0 if column == 4 else 0.0)
        for column in range(1, 5)
        for row in range(1, 5)
    ]


def is_cq_object(obj: Any) -> bool:
    return isinstance(obj, (cq.Workplane, cq.Shape, cq.Assembly, cq.Sketch))


def find_cq_objects(namespace: Mapping[str, Any]) -> dict[str, ShownObject]:
    """Every CadQuery object bound to a public name, for scripts that never
    call ``show_object``."""

    return {
        name: ShownObject(value)
        for name, value in namespace.items()
        if not name.startswith("_") and is_cq_object(value)
    }


def to_compound(obj: Any) -> cq.Compound:
    """Fuse anything shape-like into a single compound.

    Accepts workplanes, shapes, sketches, raw ``TopoDS_Shape`` objects and
    lists of those. Non-shape stack entries (vectors, locations) are skipped,
    and an empty input yields an empty compound.
    """

    shapes: list[cq.Shape] = []
    for item in obj if isinstance(obj, list) else [obj]:
        if isinstance(item, cq.Workplane):
            for value in item.vals():
                if isinstance(value, cq.Sketch):
                    shapes.extend(value)
                elif isinstance(value, cq.Shape):
                    shapes.append(value)
        elif isinstance(item, cq.Shape):
            shapes.append(item)
        elif isinstance(item, cq.Sketch):
            # Iterating a sketch yields its faces (or edges) with the sketch
            # placement applied.
            shapes.extend(item)
        elif isinstance(item, TopoDS_Shape):
            shapes.append(cq.Shape.cast(item))
        else:
            raise ValueError(f"Invalid type {type(item)}")
    return cq.Compound.makeCompound(shapes)


def parse_color(value: Any) -> tuple[Color, float | None]:
    """Normalize a user color specification to sRGB floats plus optional alpha.

    Supported: ``cq.Color``, ``"#rgb"``/``"#rrggbb"``/``"#rrggbbaa"``, color
    names known to OpenCascade (``"red"``, ``"steelblue"``, ...), and tuples
    of 3 or 4 components, either ints in 0..255 or floats in 0..1.
    """

    if isinstance(value, cq.Color):
        r, g, b, a = value.toTuple()
        return (r, g, b), a

    if isinstance(value, str):
        text = value.strip()
        if text.startswith("#"):
            digits = text[1:]
            if len(digits) in (3, 4):
                digits = "".join(c * 2 for c in digits)
            if len(digits) not in (6, 8):
                raise ValueError(f"Invalid hex color {value!r}")
            channels = [
                int(digits[i : i + 2], 16) / 255 for i in range(0, len(digits), 2)
            ]
            alpha = channels[3] if len(channels) == 4 else None
            return (channels[0], channels[1], channels[2]), alpha
        return parse_color(cq.Color(text))

    if isinstance(value, (tuple, list)) and len(value) in (3, 4):
        if all(isinstance(c, int) for c in value):
            channels = [c / 255 for c in value]
        else:
            channels = [float(c) for c in value]
        if not all(0.0 <= c <= 1.0 for c in channels):
            raise ValueError(f"Color components out of range: {value!r}")
        alpha = channels[3] if len(channels) == 4 else None
        return (channels[0], channels[1], channels[2]), alpha

    raise ValueError(f"Unsupported color specification {value!r}")


@dataclass
class DisplayOptions:
    """The ``options`` mapping accepted by ``show_object``."""

    color: Color | None = None
    #: Opacity in 0..1 (1 is opaque).
    alpha: float | None = None
    edge_color: Color | None = None

    @classmethod
    def parse(cls, options: Mapping[str, Any] | None) -> DisplayOptions:
        result = cls()
        if not options:
            return result
        try:
            if "rgba" in options:
                r, g, b, a = options["rgba"]
                result.color, _ = parse_color((r, g, b))
                result.alpha = float(a)
            if "color" in options:
                result.color, alpha = parse_color(options["color"])
                if alpha is not None and alpha < 1.0 and result.alpha is None:
                    result.alpha = alpha
            if "alpha" in options:
                result.alpha = float(options["alpha"])
            if "edgecolor" in options:
                result.edge_color, _ = parse_color(options["edgecolor"])
        except (TypeError, ValueError) as error:
            log.warning("Ignoring invalid display option: %s", error)
        if result.alpha is not None:
            result.alpha = min(max(result.alpha, 0.0), 1.0)
        return result

    def to_message(self) -> dict:
        return {
            "color": list(self.color) if self.color else None,
            "alpha": self.alpha,
            "edge_color": list(self.edge_color) if self.edge_color else None,
        }


@dataclass
class ShownObject:
    obj: Any
    options: Mapping[str, Any] = field(default_factory=dict)


@dataclass
class RegistryEntry:
    obj: Any
    #: Geometry to export for this node, already placed in world space.
    shape: cq.Shape | None
    children: list[str] = field(default_factory=list)
    #: Topological indices for each displayed part, in the same order as the
    #: scene node's parts. They keep picking ids tied to live OCCT subshapes.
    topologies: list[TopologyIndex] = field(default_factory=list)

    def subshape(self, part: int, kind: str, identifier: int) -> cq.Shape:
        try:
            topology = self.topologies[part]
        except IndexError:
            raise IndexError(f"Unknown part {part}") from None
        return cq.Shape.cast(topology.get(kind, identifier))


class Registry:
    """Keeps the CadQuery objects behind scene node keys alive."""

    def __init__(self) -> None:
        self._entries: dict[str, RegistryEntry] = {}
        self._counter = itertools.count(1)

    def add(self, entry: RegistryEntry) -> str:
        key = f"n{next(self._counter)}"
        self._entries[key] = entry
        return key

    def get(self, key: str) -> RegistryEntry:
        try:
            return self._entries[key]
        except KeyError:
            raise KeyError(
                f"Unknown object {key!r}; it may have been cleared"
            ) from None

    def release(self, keys: Iterable[str]) -> None:
        for key in keys:
            entry = self._entries.pop(key, None)
            if entry is not None:
                self.release(entry.children)

    def clear(self) -> None:
        self._entries.clear()

    def subtree_shapes(self, keys: Iterable[str]) -> list[cq.Shape]:
        shapes, seen = [], set()

        def visit(key: str) -> None:
            if key in seen:
                return
            seen.add(key)
            entry = self.get(key)
            if entry.shape is not None:
                shapes.append(entry.shape)
            for child in entry.children:
                visit(child)

        for key in keys:
            visit(key)
        return shapes


class SceneBuilder:
    def __init__(
        self,
        registry: Registry,
        tolerance: Tolerance,
        merge_assemblies: bool = False,
        geometry_cache: GeometryCache | None = None,
        known_geometry: Iterable[str] = (),
    ) -> None:
        self.registry = registry
        self.tolerance = tolerance
        self.merge_assemblies = merge_assemblies
        self.geometry_cache = geometry_cache or GeometryCache()
        # Updated as a response is built, so repeated parts in the same
        # assembly become references after the first full mesh.
        self.known_geometry = set(known_geometry)
        # Tessellation mutates OCCT's polygonal cache. Remember the signature
        # by source object so reuse within one run is unaffected by that.
        self._source_geometry: dict[int, str] = {}

    def build(self, objects: Mapping[str, ShownObject]) -> list[dict]:
        nodes = []
        for name, shown in objects.items():
            try:
                node = self.node(name, shown.obj, DisplayOptions.parse(shown.options))
            except Exception as error:
                log.error(
                    "Cannot display %s: %s: %s", name, type(error).__name__, error
                )
                continue
            if node is not None:
                nodes.append(node)
        return nodes

    def node(self, name: str, obj: Any, options: DisplayOptions) -> dict | None:
        if isinstance(obj, cq.Assembly):
            if self.merge_assemblies:
                return self._merged_assembly_node(name, obj, options)
            return self._assembly_node(name, obj, options, cq.Location(), None)

        shape = to_compound(obj)
        topology = TopologyIndex.build(shape.wrapped)
        part = self._part(shape, topology, None, None, cq.Location(), obj)
        if part is None:
            return None
        key = self.registry.add(RegistryEntry(obj, shape, topologies=[topology]))
        return self._message(key, name, obj, options, [part], [])

    def _assembly_node(
        self,
        name: str,
        assembly: cq.Assembly,
        options: DisplayOptions,
        parent_location: cq.Location,
        inherited_color: cq.Color | None,
    ) -> dict | None:
        location = parent_location * assembly.loc
        color = assembly.color if assembly.color is not None else inherited_color

        shape, parts, topologies = None, [], []
        if assembly.obj is not None:
            # A node can have both a shape and children.
            local_shape = to_compound(assembly.obj)
            shape = local_shape.moved(location)
            topology = TopologyIndex.build(shape.wrapped)
            part = self._part(
                local_shape,
                None,
                *self._part_color(color),
                location,
                assembly.obj,
            )
            if part is not None:
                parts.append(part)
                topologies.append(topology)

        children = [
            child_node
            for child in assembly.children
            if (
                child_node := self._assembly_node(
                    child.name, child, options, location, color
                )
            )
            is not None
        ]
        if not parts and not children:
            return None

        entry = RegistryEntry(
            assembly,
            shape,
            children=[c["key"] for c in children],
            topologies=topologies,
        )
        key = self.registry.add(entry)
        message = self._message(key, name, assembly, options, parts, children)
        if assembly.obj is not None:
            # More telling than "Assembly" for the parts of an assembly.
            message["kind"] = type(assembly.obj).__name__
        return message

    def _merged_assembly_node(
        self, name: str, assembly: cq.Assembly, options: DisplayOptions
    ) -> dict | None:
        parts, shapes, topologies = [], [], []

        def collect(node: cq.Assembly, parent: cq.Location, inherited) -> None:
            location = parent * node.loc
            color = node.color if node.color is not None else inherited
            if node.obj is not None:
                local_shape = to_compound(node.obj)
                shape = local_shape.moved(location)
                shapes.append(shape)
                topology = TopologyIndex.build(shape.wrapped)
                part = self._part(
                    local_shape,
                    None,
                    *self._part_color(color),
                    location,
                    node.obj,
                )
                if part is not None:
                    parts.append(part)
                    topologies.append(topology)
            for child in node.children:
                collect(child, location, color)

        collect(assembly, cq.Location(), None)
        if not parts:
            return None
        key = self.registry.add(
            RegistryEntry(
                assembly,
                cq.Compound.makeCompound(shapes),
                topologies=topologies,
            )
        )
        return self._message(key, name, assembly, options, parts, [])

    @staticmethod
    def _part_color(color: cq.Color | None) -> tuple[Color | None, float | None]:
        if color is None:
            return None, None
        rgb, alpha = parse_color(color)
        return rgb, alpha

    def _part(
        self,
        shape: cq.Shape,
        topology: TopologyIndex | None,
        color: Color | None,
        alpha: float | None,
        location: cq.Location,
        source: Any,
    ) -> dict | None:
        source_id = id(source)
        geometry_id = self._source_geometry.get(source_id)
        if geometry_id is None:
            geometry_id = self.geometry_cache.identifier(shape.wrapped, self.tolerance)
            geometry_id = self.geometry_cache.canonical_id(geometry_id)
            self._source_geometry[source_id] = geometry_id

        if geometry_id in self.known_geometry:
            mesh_message = {"id": geometry_id}
        else:
            topology = topology or TopologyIndex.build(shape.wrapped)
            _, mesh = self.geometry_cache.tessellate(
                shape.wrapped, self.tolerance, topology, geometry_id
            )
            if mesh is None:
                return None
            mesh_message = mesh.to_message(geometry_id)
            self.known_geometry.add(geometry_id)
        return {
            "mesh": mesh_message,
            "transform": location_matrix(location),
            "color": list(color) if color else None,
            "alpha": alpha,
        }

    @staticmethod
    def _message(key, name, obj, options, parts, children) -> dict:
        return {
            "key": key,
            "name": str(name),
            "kind": type(obj).__name__,
            "inspectable": isinstance(obj, cq.Workplane) and obj.parent is not None,
            **options.to_message(),
            "parts": parts,
            "children": children,
        }
