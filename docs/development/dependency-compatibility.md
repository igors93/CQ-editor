# CadQuery and OCP compatibility

The worker calls OCP directly for tessellation, topology and exact
measurements, so a successful `import cadquery` is not enough to establish
compatibility. Generated OCP names can change even when the corresponding
OCCT operation still exists.

## Supported matrix

These are the reproducible CI environments. The exact OCP patch is pinned in
CI; the package metadata uses the wider supported families so normal installs
can receive compatible fixes.

| Lane | Python | CadQuery | cadquery-ocp | Policy |
| --- | --- | --- | --- | --- |
| Legacy floor | 3.10 | 2.7.0 | 7.8.1.1.post1 | Required |
| Current stable, minimum Python | 3.11 | 2.8.0 | 7.9.3.1.1 | Required |
| Current stable, maximum Python | 3.14 | 2.8.0 | 7.9.3.1.1 | Required |
| Upstream development | 3.12 | `CadQuery/master` (currently 2.9.0dev) | 8.0.1.1.0 | Early warning |

The supported package ranges are Python 3.10–3.14, CadQuery 2.7.x–2.9.x and
OCP 7.8.1–8.0.x. Python 3.10 is paired with CadQuery 2.7 because CadQuery 2.8
itself requires Python 3.11 or newer.

The three released combinations are blocking CI jobs. The development lane
runs the same complete worker suite but is allowed to fail because it follows
a moving external branch. A failure there is still a compatibility issue to
investigate before that CadQuery/OCP family becomes stable.

The versions were selected from CadQuery's official metadata:

* [CadQuery 2.8.0](https://github.com/CadQuery/cadquery/blob/v2.8.0/setup.py)
  requires Python 3.11+ and OCP 7.9.x;
* [CadQuery `master`](https://github.com/CadQuery/cadquery/blob/master/setup.py)
  currently declares 2.9.0dev, Python 3.11+ and OCP 8.0.1.1;
* [OCP's build matrix](https://github.com/CadQuery/ocp-build-system/blob/main/.github/workflows/build-ocp.yml)
  provides the platform-specific Python wheels.

## What the gate exercises

`test_compatibility.py` checks that the selected versions match the intended
lane and performs a real box tessellation with topology identifiers. The
complete worker suite then covers execution, assemblies, inspection, GLB
conversion, exact measurements and exports.

The tessellation suite also compares the display mesh with a separate
face-by-face OCCT extraction across ten shape families. An injected glTF face
reordering preserves every triangle count and verifies that geometric
validation activates the independent pick-mesh fallback.

OCP 8 moved generated collection types to `OCP.collections` and removed the
`_s` suffix from the `TopoDS` downcasts. `tessellation.py` selects those two
spellings at import time. Geometry behavior is shared; there is no forked
tessellation implementation.

Every pytest run prints Python, implementation, CadQuery, OCP, platform and
the interpreter path in its header. The worker sends CadQuery and OCP versions
in `ready`, so application logs and timing reports can be tied to the same
environment. The ignored Rust picking benchmark prints its own application
version, OS, architecture, build profile and logical CPU count.

When a stable CadQuery or Python release changes, add its exact combination
to `.github/workflows/ci.yml`, run the entire worker suite, and only then move
the documented range or remove an older lane.
