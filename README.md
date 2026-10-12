# Headless CAD

**Write a recipe. Build a part. Inspect it. Export it.**

Our goal is an **Apache-2.0-licensed, headless, AI-native CAD system with an
original Rust kernel**, targeting commercial engineering workflows served by
Autodesk-class platforms. This repository is the initial working foundation;
its current bounded geometry support is not the product's target scope.

Read the [target architecture](docs/architecture.md) and
[milestone roadmap](docs/roadmap.md). First-party code is licensed under
[Apache-2.0](LICENSE); dependencies retain their own licenses and notices.
See the [dependency license inventory](docs/dependency-licenses.md).

A local, CLI-first CAD product built around our own Rust B-rep kernel. Recipes
are plain text suitable for Git, results are inspectable JSON, and exports are
STEP and STL. Modeling currently supports **orthogonal solids and simple polygon extrusions, including nonconvex
outlines and simple polygonal through-holes, plus true circular extrusions, simple concave line/arc profiles and stepped/tapered revolutions**: slotted brackets, open trays, triangular ramps, and polygonal
seating blocks. Convex solids also support native rigid transforms. Reusable
assemblies can place these parts with rigid rotations;
Union, cut and intersection support orthogonal solids and a bounded analytic
branch for two parallel solid cylinders. Right prisms and circular/annular stock
also accept contained through/blind drilling and counterbores. Intersection supports bounded,
well-conditioned convex planar solids, including rotated inputs.

No existing CAD kernel is linked at runtime. Open CASCADE is used only as an
optional independent interoperability oracle in development.

## Get started

Install Rust using rustup and a system C compiler/linker, then:

```sh
cargo install --path crates/cad-cli --locked
cad init my-part
cad build my-part/main.star --out-dir my-part/build
```

Open `my-part/build/plate.preview.html` in a browser. It is entirely local and
self-contained: drag to orbit, scroll to zoom, click a face to inspect it, or use
the named view buttons. The SVG preview is useful in reviews and documentation.
The portable inspector lists native faces separately from display patches;
assembly native-face totals sum occurrences of shared definitions.

**Recipes/code are the source of truth. The viewer is read-only.** It has no
modeling tools, kernel calls, or write-back path. Every geometry change is made in
the recipe and evaluated through the CLI/Rust API. Preview files can be discarded
and regenerated without losing any design information.

To run directly from the repository, replace `cad` with
`cargo run --locked -p cad-cli --` in the commands below. Build a distribution
archive for the current platform with `./scripts/package.sh`.
For an unpacked archive, use its included `./cad` executable instead. Native
archives target their build platform and libc; source installation is the
portable route across operating systems.

## Discover operations for automation

```sh
cad capabilities
cad capabilities --operation feature.cut
cad capabilities --type assembly_definition
cad compile examples/machined_bearing_support.star
cad check examples/machined_bearing_support.star
```

`capabilities` returns a deterministic, versioned JSON catalog without reading a
recipe, evaluating geometry or creating a cache. It identifies the running
catalog/kernel/executor/assembly/solver implementations, supported recipe schema versions,
operation arguments, the recipe envelope, nested profile/sketch types, assemblies,
attachments, materials, face references, engineering assertions, geometry domains,
exclusions and resource limits.
Rust clients use `cad_build::capability_catalog()` for the same modeling catalog;
the CLI adds command and authoring-workflow metadata. Neither `--param` nor
`--cache-dir` applies to discovery.

Argument names describe the **JSON recipe IR**, not Starlark signatures. Use
`compile` to inspect the IR of a source recipe. The catalog is a planning aid,
not a complete JSON Schema or a guarantee that a particular geometry will work;
`check` evaluates the supported domain and all validation obligations. The scope
covers the current recipe argument structure; release policies and a hosted agent
protocol remain future work. Catalog schema 2 introduces map and nullable types,
and marks fields restricted to particular recipe schema versions. Catalog and
recipe schema versions are separate contracts.

Use `--operation ID` or `--type ID` to retrieve a focused contract and its nested
inline types. Logical references remain IDs rather than expanding referenced
objects. Global limits and implementation identities remain present; the `scope`
field distinguishes full and focused CLI reports.

Runtime failures return JSON on stderr and a nonzero exit status. Native operation
failures retain `code: "kernel_operation_failed"` and a `feature` identifier,
and now include diagnostic text and implementation identities. The diagnostic remains explanatory text; callers
should dispatch on structured fields rather than parse it. CLI usage/parser
errors still use human-readable stderr. Successful commands return JSON on stdout.

An agent proposes a source patch, checks it, inspects the revision and publishes
with `build`; it does not issue modeling calls through the viewer. Acceptance
replays a proposed fully enclosed cylindrical cut, checks its feature-specific
failure, applies an explicit source correction, and rebuilds matching native/STEP/STL artifacts
in fresh processes. This verifies the headless protocol and deterministic replay,
not an AI model's reasoning or a completed autonomous design benchmark.

## Serve and watch locally

From a source checkout, one command builds all examples, watches their recipes,
and starts the gallery server:

```sh
./scripts/dev.sh --host 0.0.0.0 --port 8080
```

Ctrl-C stops the server and all watchers together. Set `CAD_BIN` to use an
already-built executable. Individual commands are also available below.

After building the examples, serve their previews and exports:

```sh
cad serve out --host 0.0.0.0 --port 8080
```

Open `http://localhost:8080` for the model gallery. When the process runs on a
remote development machine, forward port 8080 or use that machine’s reachable
address. The default host is `127.0.0.1`; `0.0.0.0` listens on all interfaces.
The server exposes only the chosen artifact directory and has no modeling or
recipe-editing endpoints.

Keep a recipe building as you edit it in another terminal:

```sh
cad watch examples/bracket.star --out-dir out/bracket --cache-dir .cad-cache
```

A successful rebuild updates the hosted preview automatically while preserving
the camera. Invalid edits report an error and leave the last successful model
available. Fixing the recipe resumes rebuilding. Stop either process with Ctrl-C.
Standalone `file://` previews remain usable without a server, with manual reload.

Choose **Recipe** in a part’s viewer to inspect the exact source used to build
that generation, its output/feature identifiers, and parameter overrides. This
is a read-only view of the complete recipe, including shared helper functions;
it is not a reconstructed recipe inferred from geometry. Starlark builds also
export `recipe.star`; `recipe.json` remains the resolved graph. Source-only edits
refresh the recipe panel even when the geometry is unchanged.

The **Engineering checks** panel shows captured native property assertions,
including measured values, inclusive bounds and the output each check applies
to. Expand a check for its target content hash. Checks on other build outputs
remain labeled as such when inspecting a selected part. These are published
build results, not measurements of the display mesh; picking never runs a query.
Canvas refreshes the checks with the published generation. GPU views require
a reload after rebuilding.

## Build useful examples

```sh
cad build examples/bracket.star --out-dir out/bracket
cad build examples/tray.star --out-dir out/tray
cad build examples/intersection.star --out-dir out/intersection
cad build examples/fixture.star --out-dir out/fixture
cad build examples/enclosure.star --out-dir out/enclosure
cad build examples/mounting_array.star --out-dir out/mounting_array
cad build examples/reference_fixture.star --out-dir out/reference_fixture
cad build examples/modular_rack.star --out-dir out/modular_rack
cad build examples/angled_fixture.star --out-dir out/angled_fixture
cad build examples/datum_gantry.star --out-dir out/datum_gantry
cad build examples/perforated_mount.star --out-dir out/perforated_mount
cad build examples/fork_mount.star --out-dir out/fork_mount
```

| Example | What it exercises | Independently calculated result |
| --- | --- | --- |
| Slotted bracket | Overlapping union, base slot, wall window | 24,624 mm³, one solid, 80 × 40 × 40 mm |
| Open tray | Open pocket and through-drain | 53,616 mm³, one solid |
| Tray and separate lid | Disconnected assembly | 74,616 mm³, two solids |
| Intersection | Common material of overlapping boxes | 2,250 mm³, one solid |
| Modular fixture | Loop-generated slots, attached fence, separate jaw stops | 89,856 mm³, three solids |
| Vented enclosure | Eight vents, four attached internal supports, separate lid | 88,240 mm³, two component solids |
| Mounting array | One slotted part, 24 instances with alternate rows rotated | 4,992 mm³ summed material, one stored part definition |
| Reference fixture | Three grooves and raised stops; ancestral top-face selection | 23,296 mm³, one solid, 80 × 40 × 20 mm |
| Modular rack | Reused rows and carriages, nested rotations and occurrence paths | 63,648 mm³ summed material, 17 component occurrences, four part definitions |
| Angled fixture | Two rotated triangular-prism ramps, four hexagonal seats, and an orthogonal slotted base | 47,346.83063257983 mm³ summed material, seven occurrences; each ramp is 12,000 mm³ |
| Fork mount | U-shaped tooling plate with arm openings and an offset cap datum | Plain blank 22,500 mm³; opened fork 21,096 mm³; assembly 41,324 mm³ |
| Perforated mount | Octagonal plate with two polygonal openings and a locator following the remaining cap centroid | Plate 20,100 mm³; seven-part assembly 33,428 mm³ |
| Datum gantry | Sibling datum attachments for posts, bridge, tool and clamps; two repeated stations | 80,440 mm³ and seven occurrences per gantry; inspection line doubles both |

The examples progress from single-part modeling to reusable functions, repeated
features, and assemblies. The fixture and enclosure recipes expose named
parameters and use loops instead of copying feature definitions. Their parts
are also exported separately for inspection. Future kernel tools should ship
with a more demanding example and independent expected measurements.

These values are stored in `examples/expected.json` and checked by acceptance
tests. Boolean face counts are intentionally not fixed: subdivision may change
while the engineering geometry remains equivalent.

Each output includes:

- `<name>.step`: planar solid CAD interchange, with explicit millimetre units.
- `<name>.stl`: outward-oriented triangles for downstream mesh workflows.
- `<name>.brep.json`: topology, supporting geometry, and operation history.
- `<name>.preview.html` and `<name>.preview.svg`: interactive and static previews.
- `manifest.json`: measurements, validation, provenance, hashes, and artifact list.
- `recipe.star`: the original Starlark source for Starlark builds.
- `recipe.json`: the resolved recipe graph, including parameter overrides, for rebuilding.

## Parts and assembly instances

Assemblies preserve component identity and reuse their geometry. The fixture,
enclosure, and mounting-array examples use part definitions and rigid instances:

```python
profile = rectangle(id = "profile", width = 10, depth = 6)
body = extrude(id = "body", profile = profile, height = 4)
definition = part(id = "block", body = body)
first = instance(id = "first", part = definition)
second = instance(id = "second", part = definition, offset = [20, 0, 0],
                  rotation = [0, 0, 0.7071067811865476, 0.7071067811865476])
pair = assembly(id = "pair", instances = [first, second])
assembly_output(name = "pair", assembly = pair)
```

Rotations are unit quaternions in `[x, y, z, w]` order; translations are
millimetres. A placement rotates a reusable part without changing its local
B-rep. Modeling across placed instances requires explicit kernel operations;
`transform` and scoped convex `intersect` operate on exact part geometry.
Parts must reference solid features. Assemblies support both part occurrences
and nested subassembly occurrences, with named part datums and deterministic
sibling, descendant and whole-module attachments. Joints and nonlinear constraint solving remain planned.

These functions produce recipe schema 2. Existing single-part recipes retain
schema 1; their JSON and Starlark workflows remain supported. A part's local
geometry uses the ordinary content cache. Changing only placements reuses that
geometry, while updating the assembly identity and conservative world bounds.

Use `assembly_instance` to place an existing assembly inside another:

```python
left = assembly_instance(id = "left", assembly = carriage)
right = assembly_instance(id = "right", assembly = carriage, offset = [48, 0, 0])
row = assembly(id = "row", instances = [left, right])
```

An assembly can mix part and subassembly occurrences. Local placements compose
from parent to child; repeated modules share definitions. IDs must be unique
within a definition, and the Starlark declaration registry also requires unique
declared instance IDs. Different occurrence paths distinguish repeated children.
Nested leaf IDs encode their complete path as a JSON array; `instance_paths`
retains the original path segments, so slashes in an ID cannot create collisions.
The viewer displays those paths when selecting parts.

### Named datums and rigid attachment

Named frames can be fixed or derived from one semantic face. For example:

```python
ramp = part(id = "ramp", body = ramp_body, datums = {
    "seat": face_datum(from_feature = prism, role = "wall_1",
                       x_direction = [1, 0, 0]),
})
```

The selector follows feature lineage to the owning part's body and must resolve
to exactly one face. The frame starts at its area centroid, uses the outward
normal as Z, and projects the explicit `x_direction` into the plane for X.
Optional `offset = [x, y, z]` is in this local frame. Split/deleted faces,
parallel or ill-conditioned hints and numerically unreliable frames fail before
publication. No automatic retargeting or axis fallback occurs.

Source intent is retained in schema-2 `face_datums`; inspect/build reports retain
selection evidence and resolved frames. Existing attachments use those frames.
See `examples/surface_mount.star`: changing `ramp_rise` moves and turns its
attached plate. STEP preserves the resulting geometry/placements.


A datum is a named local frame on a part definition. `datum()` defaults to zero
translation and identity quaternion; `offset` is in millimetres and `rotation`
is a unit quaternion `[x, y, z, w]`. Define frames in source and attach a sibling
part occurrence by naming its own datum and the target's datum:

```python
profile = rectangle(id = "profile", width = 10, depth = 6)
body = extrude(id = "body", profile = profile, height = 4)
block = part(id = "block", body = body, datums = {
    "foot": datum(),
    "top": datum(offset = [0, 0, 4]),
})
base = instance(id = "base", part = block)
upper = instance(id = "upper", part = block, attach_to = "base",
                 datum = "foot", target_datum = "top", offset = [0, 0, 2])
stack = assembly(id = "stack", instances = [upper, base])
assembly_output(name = "stack", assembly = stack)
```

The upper block is placed at Z = 6 mm. For an attached instance, its `offset`
and `rotation` describe an additional local frame relative to the target datum,
not a world-space placement. Using transform composition from right to left:

```text
placed = target_placement × target_datum × instance_offset × inverse(own_datum)
```

The inverse own datum matters when that frame is translated or rotated. Without
attachment arguments, instance transforms retain their ordinary placement
meaning. All three attachment arguments (`attach_to`, `datum`, `target_datum`)
are required together. With no `target_path`, targets are sibling **part occurrences
in the same assembly definition**. To target a part inside a child assembly,
use `attach_to` for the child occurrence and `target_path` for the remaining
occurrence IDs, ending at the target part:

```python
sensor = instance(id = "sensor", part = sensor_part,
    attach_to = "cartridge", target_path = ["core", "housing"],
    datum = "foot", target_datum = "sensor_seat")
```

Path components are literal IDs, including any punctuation; slashes are not
separators. Paths descend from the containing assembly, never to parents or
another output. They select occurrences, not shared definition IDs. Each
reusable child definition resolves once before its parent; terminal part
placements include their own attachments. The composed target placement is
relative to the source instance’s containing assembly. Paths are limited to
64 components including `attach_to`. Definition order does not control
evaluation: references form a dependency DAG, with missing targets/datums
and cycles rejected. Sibling chains
are limited to 64 occurrences, with at most 1,024 named frames per part and the
existing assembly expansion/work limits. Invalid unit frames and placements
outside numerical tolerance fail explicitly; rotations are never silently
normalized.

Frames and attachment intent remain in the recipe/native assembly hierarchy;
derived leaf placements and STEP occurrence transforms contain the resolved
placement. Native validation recomputes it and rejects inconsistent derived
data. STEP does **not** preserve named datum or attachment semantics themselves.
Datum edits update assembly identity and dependent placements while reusing
unchanged part geometry. Selected-part metadata exposes the local datum frames
in the read-only viewer. See `examples/datum_gantry.star` for chained attachments,
nonzero/rotated tool datums, and reuse of the completed gantry in two stations.

These are deterministic placements; named frames can also be associated with
supported planar faces using `face_datum`. `instrumented_cartridge.star` shows
a sensor attached inside a reused, nested cartridge. Moving the cartridge
repositions the sensor without rebuilding geometry. Upward references, multiple
simultaneous mates, joints, DOF/residual reporting and collision checking remain
unsupported.

To attach a **whole rigid subassembly**, identify its mounting datum with a
nonempty `source_path` relative to that subassembly:

```python
mounted = assembly_instance(id = "mounted", assembly = drive_module,
    attach_to = "base", source_path = ["cartridge", "housing"],
    datum = "mount", target_datum = "dock")
```

All four arguments (`attach_to`, `source_path`, `datum`, `target_datum`) are
required together; optional `target_path` selects a descendant on the target
side. Both paths end at part occurrences with named datums. The module moves
as one rigid unit; its internal placements and shared geometry are unchanged.
Its placement aligns the complete resolved source datum frame:

```text
module_placement = target_frame × instance_offset × inverse(internal_source_frame)
```

Parts and modules participate in one local attachment DAG. A target within the
source module itself is a cycle, as is a module attached to a part that follows
that module. Both are rejected before modeling. Paths and dependency chains
remain limited to 64 components/occurrences. `mounted_drive_module.star` docks
two copies of a nested drive module onto base datums, then attaches a sensor
through one placed module. Editing a docking datum repositions the module and
sensor with no solid geometry recomputation.

Assembly builds produce `<name>.assembly.json`, a metadata manifest referencing
shared `definition-<hash>.brep.json` assets, and `<name>.display.json`, with one
local mesh per geometry hash plus occurrence transforms. The HTML viewer embeds
the compact display representation and expands it for this software renderer.
It does not load exact B-rep assets to display an assembly. Select an occurrence
or face to inspect its part, feature, and the actual shared recipe.

The native manifest retains `hierarchy` alongside derived leaf placements;
validation rejects disagreement between hierarchy, leaf paths, and transforms.
STEP exports preserve reusable part and subassembly definitions with local named
occurrence placements. STL
expands occurrences into triangles and loses assembly semantics. Assembly volume
is explicitly **summed component material**, including overlaps; it is not a
Boolean-union volume or an interference certificate. Bounds are conservative
axis-aligned bounds of placed part bounds.

```sh
cad validate out/mounting_array/mounting_array.assembly.json
cad inspect examples/mounting_array.star --param columns=12 --cache-dir .cad-cache
```

Validation loads the referenced exact definitions, checks placement and build
identity, and checks the manifest's measured metadata. Assembly hashes currently
bind the producing implementation; retain that executable or rebuild the recipe
after an implementation update. STEP remains independent of this native hash. The current display limit
is 10,000 occurrences, 50,000 expanded faces, and 200,000 points; the modeling
assembly limit and placement-work budget are separate. GPU instancing and lazy
streaming remain future scalability work, not claims about this Canvas renderer.
Hierarchy validation rejects cycles, missing references, excessive depth, and
exponential expansion before allocating placed geometry. Definitions are limited
to 64 levels, with at most 100,000 expanded component and assembly occurrences
per root. Nested placement is not an assembly constraint solver or interference
check; the current viewer still expands all visible-generation leaves eagerly.

## Materials and bill of materials

Part definitions can carry engineering metadata. Densities are explicit recipe
inputs in kg/m³, not guessed from a material name:

```python
guide = part(id = "guide", body = guide_body, part_number = "RACK-GUIDE",
             revision = "A", description = "Reversible guide",
             material = "Demo steel", density_kg_per_m3 = 7800)
```

`material` and `density_kg_per_m3` must be supplied together. The modular rack uses
illustrative densities; replace them with the intended material specification.

```sh
cad bom examples/modular_rack.star --assembly modular_rack
```

The BOM expands nested quantities and groups by logical part ID. Identical
geometry, part numbers, or revisions do not merge distinct definitions. The rack
contains eight guides, four plates, four sliders, and one base. Metadata edits
reuse geometry but change assembly identity and appear in `cad diff`.

Assembly builds write `<name>.bom.json` and `<name>.bom.csv`, linked from the
viewer gallery. Reports include per-part and summed volume/mass. Mass is
`volume_mm3 × 1e-9 × density_kg_per_m3`; overlaps remain counted per occurrence.
If any occurrence lacks a material, total mass is null, with known mass and the
missing-material count reported separately. CSV uses empty unknown numeric
fields and preserves quoted text; import textual columns as text in spreadsheet
tools. The viewer shows selected part metadata and summed mass.

Native assembly artifacts retain metadata. STEP currently preserves assembly
geometry and structure, but does not carry these material or revision fields;
use the accompanying BOM. Materials do not yet include certified property
libraries, occurrence overrides, configurations, or mass moments/inertia.

## Mass properties

Query the authoritative solids without opening a viewer or publishing artifacts:

```sh
cad properties examples/balance_arm.star --output balance_arm
cad properties examples/balance_arm.star --output balance_arm --param counterweight_distance=45
cad properties examples/balance_arm.star --output beam --density-kg-per-m3 2700
```

Reports contain volume, volume centroid and central volume inertia, plus mass,
center of mass and central mass inertia when density is known. Volume units are
mm³; geometric inertia is mm⁵; mass is kg; mass inertia is kg·mm². Tensors use
`∫ (|r|² I − r rᵀ)` about their corresponding centroid, so off-diagonal entries
are **negative** product moments. Directions use the output model frame or root
assembly frame, as labeled in the report. Source/parameter and build fingerprints
identify the queried snapshot.

For a body output, the optional density flag supplies a uniform material density.
Assemblies use each part's material metadata and reject a uniform override. If
any occurrence lacks density, total mass, center of mass and mass inertia are
`null`; geometric properties remain available. This includes empty occurrences,
matching the BOM completeness policy. Empty geometry has no centroid or tensor.
Overlapping assembly occurrences count separately as material, not as a union.

The balance arm uses five boxes, rigid datum attachments and mixed densities.
Its default mass is **0.115776 kg**. Its volume centroid is approximately
`[7.707626, 4.45, 31.45] mm`, while its center of mass is
`[1.357204, 0.783582, 33.365672] mm`. Moving the counterweight changes placement
and the inertia tensor without changing the part shapes. Independent box-moment
expectations cover three positions in `balance_arm.properties.expected.json`.

The kernel integrates signed tetrahedral domains from planar boundary faces;
these are not viewer triangles. Local references, compensated sums and a centered
second pass reduce cancellation. Unrepresentable centroids, overflow, underflow
and excessive cancellation return explicit numerical errors. The command does
not infer density, solve dynamics, or calculate principal axes yet.

## Recipes and parameters

```python
def box(id, width, depth, height):
    profile = rectangle(id = id + "/profile", width = width, depth = depth)
    return extrude(id = id, profile = profile, height = height)

width = param(name = "width", default = 80)
base = box("base", width, 40, 6)
wall = box("wall", 6, 40, 40)
bracket = union(id = "bracket", left = base, right = wall)

tool = box("tool", 20, 10, 8)
positioned = translate(id = "positioned", body = tool, offset = [30, 15, -1])
finished = cut(id = "slotted", left = bracket, right = positioned)
output(name = "bracket", feature = finished)
```

| Function | Meaning |
| --- | --- |
| `rectangle(id, width, depth)` | Profile on XY, starting at the origin |
| `polygon(id, points, holes=[])` | Simple CCW outer XY ring (convex or nonconvex) with optional simple CW holes; 256 total vertices, at most 32 holes |
| `path(id, start, segments, circular_holes=[])` | Closed simple CCW line/arc profile, including concave recesses; signed arcs up to a half-turn and circular through-holes |
| `sketch(id, points, constraints, outline, holes=[])` | Dimension-driven polygon profile; named point guesses, fixed/coincident/horizontal/vertical/distance/equal-length/parallel/perpendicular/directed-angle constraints; requires a converged, locally fully constrained solution |
| `extrude(id, profile, height)` | Extrude a supported rectangle, polygon, circle or line/arc profile along positive Z |
| `translate(id, body, offset)` | Translate by `[x, y, z]`; named `x`, `y`, `z` also supported |
| `transform(id, body, offset, rotation)` | Rotate exact geometry using unit quaternion `[x,y,z,w]`, then translate; both default to identity |
| `union(id, left, right)` | Combine orthogonal material |
| `cut(id, left, right)` | Remove right-hand orthogonal material from the left |
| `intersect(id, left, right)` | Keep common material: orthogonal solids or two supported convex planar solids |
| `param(name, default)` | Declare a numeric parameter the CLI may override |
| `output(name, feature)` | Declare a solid result to inspect or export |

Outer polygon rings must be finite, simple, and counterclockwise; nonconvex
outlines such as L and U shapes are supported. Do not
repeat the first point at the end. Repeated points, crossings, collinear turns,
edges at/below tolerance, and numerically uncertain turns are rejected rather
than repaired. The exact B-rep retains one planar polygon face per cap and one
quadrilateral per side; display and STL triangles are derived from those faces.
Optional `holes` are simple **clockwise** rings strictly inside the outer
boundary. Holes must be disjoint, nonnested and separated by more than the
numerical/tolerance envelope; touching or uncertain arrangements fail explicitly.
There are at most 32 holes and 256 total profile vertices. Caps retain their
inner loops in the exact B-rep and STEP; derived STL/display triangles leave
openings empty. Self-crossings, nonadjacent edges within the numerical/tolerance envelope,
and holes that cross a concave bay are rejected. Both outer and inner rings may be nonconvex.

```python
profile = polygon(id = "frame_profile",
    points = [[0, 0], [40, 0], [40, 30], [0, 30]],
    holes = [[[10, 10], [10, 20], [30, 20], [30, 10]]])
frame = extrude(id = "frame", profile = profile, height = 5)
```

Outer edge roles remain `wall_i`; inner edges use `hole_i_wall_j` in source ring
order. `walls` includes all walls and `hole_walls` selects only opening walls.
Cap roles still resolve to one face each. Measurements and face-derived datums
subtract the holes: the cap's area centroid need not lie on remaining material.
See `perforated_mount.star` for two openings and an attached centroid locator.
`fork_mount.star` adds a U-shaped outer boundary and two openings in its arms;
its locator uses an explicit datum offset to remain on the front web even when
the remaining cap's area centroid lies in the empty notch. Unsplit caps remain
native faces; triangulation does not introduce feature edges or new face IDs.
[serpentine_manifold.star](examples/serpentine_manifold.star) combines a nonconvex
clockwise channel opening with four circular fastener bores, a floor, ported
cover, two shared fittings and four shared bolts: nine occurrences from five
part definitions. The `open_channel` inspection configuration exposes the passage.
`channel_width` and `channel_depth` edits each rebuild two definitions; volume
and mass assertions remain headless build gates. Independent rectangle/cylinder
subtraction checks volume and the full inertia tensor. The example expresses
channel and seating intent without a pressure, seal or interference rating.

Holed prisms do not support the general convex Boolean intersection path.
All build renderers use the recipe's tolerance. Holed preview scenes use a
versioned embedded marker; older already-open Canvas clients retain their last
valid scene and require a reload after this upgrade.

```python
profile = polygon(id = "ramp_profile", points = [[0, 0], [40, 0], [0, 20]])
ramp = extrude(id = "ramp", profile = profile, height = 30)
output(name = "ramp", feature = ramp)
```

`transform` changes the native solid coordinates, line directions and plane
normals; STEP/STL therefore contain the transformed part itself. It applies
`R * point + offset`. Assembly `instance` placement remains a separate operation
that reuses a part definition. Neither operation accepts scale, reflection or
implicit quaternion normalization.

General rotations support one strictly convex solid, one validated simple-profile
right prism, or an empty body.
Exactly represented axis-permutation matrices also preserve the orthogonal
nonconvex/multiple-solid domain. Near-axis rotations are not snapped: even a
nominal quarter turn expressed with floating-point square roots may require the
convex path. Rotated convex intersection is supported within the limits below; general
rotated union and cut remain unsupported. Numeric
range and post-transform validity are checked before publication.

```python
upright = transform(id = "upright", body = ramp,
    rotation = [0.7071067811865476, 0, 0, 0.7071067811865476])
placed = transform(id = "placed", body = upright, offset = [10, 20, 4],
    rotation = [0, 0, 0.25881904510252074, 0.9659258262890683])
```

`examples/tilted_jig.star` chains these operations into two angled tools on a
slotted base. Semantic cap/wall assertions follow the original extrusion through
both transforms. The default jig has three solids and 18560 mm³ of material;
parameter-edit tests check tool vertices against independent scalar formulas.

Convex `intersect` reconstructs analytic polygon faces from the operands' plane
half-spaces. It retains face ancestry from both inputs; exactly coincident
supporting faces can contribute to the same result face. It currently accepts
at most 48 input planes, with two single strictly convex solids (or an empty
operand). Near-dependent plane systems, unresolved near-coincidence and
lower-dimensional contact are rejected explicitly. The existing orthogonal
intersection path retains its broader nonconvex/multiple-solid support.

`examples/clipped_tools.star` intersects a rotated square prism with a rectangular
crop, and a tilted solid with a box to form two sloping shoulders. Independent
cross-section formulas give 900 and 2750 mm³ respectively; parameter edits check
both volumes and semantic references from each operand. The assembly's material
volume is 8900 mm³. General nonconvex planar Booleans remain future work.

All lengths are millimetres. `mm(value)` is a numeric annotation, not
dimension-aware arithmetic. Feature references are strings; the graph builder
checks their types. Namespace IDs inside reusable functions to avoid collisions.

```sh
cad build examples/bracket.star --param width=100 --out-dir out/wide-bracket
cad inspect examples/bracket.star --param width=100
cad compile examples/bracket.star --param width=100 > wide-bracket.json
cad check wide-bracket.json
cad diff examples/bracket.star wide-bracket.json
```

Unknown, duplicate, or nonfinite parameter overrides are errors. Overrides apply
to Starlark recipes; compiled JSON contains resolved parameters. JSON and
Starlark feed the same strict, versioned graph. Plain-text diffs capture design
intent; `cad diff` reports output changes, dimensions, and volume deltas. Its
hashes identify build inputs, not geometric equivalence.

## CLI and agent workflow

```sh
cad init <directory>
cad compile <recipe.star|recipe.json>
cad inspect <recipe.star|recipe.json>
cad properties <recipe.star|recipe.json> --output <name> [--density-kg-per-m3 <value>]
cad check <recipe.star|recipe.json>
cad build <recipe.star|recipe.json> --out-dir <directory>
cad validate <model.brep.json|assembly.assembly.json>
cad diff <before.star|before.json> <after.star|after.json>
cad select <recipe.star|recipe.json> --from <feature> --role <role> --in <feature> [--count N | --all]
```

Commands emit JSON. Failures return a nonzero status with a JSON error on stderr;
geometry errors identify the feature, and serialized-model validation returns
individual issues. An agent can edit a recipe, build it, inspect measurements,
and compare results without interacting with a GUI. Starlark syntax/native-call
errors include source locations; later build errors identify feature IDs rather
than exact source spans.

### Semantic face queries

`cad select` follows an original feature's generated face role through the
logical feature graph. Translations preserve face ancestry; orthogonal Booleans
record splits, deletions, and contributions from both operands. For example:

```sh
cad select examples/reference_fixture.star --from blank --role end_cap --in finished --all
```

This selects the surviving patches of the plate's original top, excluding the
groove floors and raised stop tops. The default cardinality is one: omit `--all`
on this example and the command reports `ambiguous`, with candidates, instead of
silently choosing a patch. `--count N` requires exactly N faces. `--all` accepts a
nonempty set; it does not turn complete deletion into success.

Results include selector intent and snapshot-local face witnesses with operation
lineage, source and feature hashes, parameters, tolerance, and kernel build ID.
Nonresolved queries return nonzero with structured `ambiguous`,
`deleted`, or `unsupported` diagnostics. Unknown roles, unrelated feature IDs,
and missing lineage are unsupported, not deletion. Identical cached geometry
does not make two logical feature IDs interchangeable. Queries accept `--param`
and `--cache-dir`, without publishing geometry or modifying the recipe.
Selection has a one-million-unit traversal/copy budget, including copied identifier bytes; exhaustion reports
`unsupported` and returns no certified partial witnesses.

Polygon extrusions generate `start_cap`, `end_cap`, `walls`, and `wall_0` through
`wall_<n-1>`. `wall_i` comes from the input edge from vertex `i` to vertex
`(i + 1) % n`; reordering the profile changes this role intent. Translation
preserves those roles through lineage. Supported convex intersections and
contained through-bores also preserve source-face lineage; general polygon
Boolean lineage remains unsupported.

This is the initial scoped face-reference contract. It does not yet provide
edge/vertex selectors, adjacency predicates, topology-associated datums,
assembly-instance selectors,
or references consumed by downstream sketch/fillet operations. Keep semantic
intent in source; numeric face IDs in witnesses belong only to the reported
build snapshot. B-rep patch counts can change after edits, even when a visible
surface remains connected.

### Design assertions in recipes

Persist required face selections in Starlark so every build checks design intent:

```python
assert_faces(id = "two_stop_tops", from_feature = stop, role = "end_cap",
             in_feature = finished, count = 2)
assert_faces(id = "plate_top_survives", from_feature = blank, role = "end_cap",
             in_feature = finished, all = True)
```

`count` defaults to one; `all=True` requires a nonempty selection and cannot be
combined with an explicit count. Each assertion needs a unique ID. Assertions
use recipe schema 2 and are included in compiled JSON. Builds evaluate them
after geometry, including on cache hits, and publish only when every assertion
resolves. Failure returns `face_assertion_failed`, the assertion ID, status,
candidate witnesses, and geometry fingerprints. The previous published
generation remains intact, including in watch mode.

Successful `check` and `build` reports contain `face_assertions`; exported build
manifests retain those witnesses with source and build identity. `cad diff`
reports assertion changes even when geometric outputs are unchanged. These are
topological-intent checks, not dimension, clearance, or stress assertions.
There is a limit of 1,000 assertions per recipe, sharing one million work/copy
units across the complete assertion set. Exhaustion fails the build explicitly.

Recipes can also enforce engineering limits on named body or assembly outputs:

```python
assert_property(id = "mass_budget", output = "mounted_drive_module",
                property = "mass_kg", max = 0.65)
assert_property(id = "balance", output = "mounted_drive_module",
                property = "mass_centroid_x_mm", min = 45, max = 61)
```

`assert_property` uses schema 2 and requires at least one finite, inclusive bound.
When both are given, `min <= max`. There is no implicit percentage or comparison
tolerance: express the acceptable interval explicitly in the property's units.
Measurements come from native geometry and material metadata, never display LOD.

| Property | Meaning |
| --- | --- |
| `volume_mm3` | Body volume or summed assembly occurrence material volume |
| `volume_centroid_x_mm`, `_y_mm`, `_z_mm` | Volume-weighted centroid in the output frame |
| `volume_inertia_xx_mm5`, `_yy_mm5`, `_zz_mm5`, `_xy_mm5`, `_xz_mm5`, `_yz_mm5` | Central volume inertia tensor components |
| `mass_kg` | Assembly mass; every occurrence must have density |
| `mass_centroid_x_mm`, `_y_mm`, `_z_mm` | Assembly centre of mass |
| `mass_inertia_xx_kg_mm2`, `_yy_kg_mm2`, `_zz_kg_mm2`, `_xy_kg_mm2`, `_xz_kg_mm2`, `_yz_kg_mm2` | Central mass inertia tensor components |

Tensor off-diagonals use negative product moments. Assembly sums count overlaps;
they are not Boolean-union measurements or collision checks. Body outputs carry
no density, so mass-weighted assertions require an assembly output. Undefined
centroids/inertias and missing densities fail as `unavailable`, rather than zero.

All property assertions run even for `build --output ...`, `--display-only`, and
cached builds. An unrequested asserted output is evaluated for validation but
is not published. Reports and the build manifest retain `property_assertions`
with bounds, actual values and target content hashes. Failed checks return
`property_assertion_failed` with `out_of_range` or `unavailable`; the previous
generation stays intact. `cad diff` reports changed assertion limits even when
the geometry stays identical. `mounted_drive_module.star` exercises fixed mass,
balance and material-volume limits across docking edits.

There are at most 1,000 property assertions. Each distinct target is measured
once per build, with a ten-million-unit aggregate topology/occurrence budget
reserved before property queries. Exhaustion fails explicitly. These checks do
not certify fit, clearance, stress, manufacturing tolerances or solver convergence.

`cad init` never overwrites an existing recipe. Builds stage all artifacts before
publication. Linux replacement uses atomic directory exchange. Other platforms
use a rename/rollback fallback with a brief publication gap. Only empty
directories or directories containing exclusively artifacts listed in their CAD
manifest may be replaced. Unmanaged files are preserved by refusing replacement.
Output names must be safe, noncolliding filenames.

### Verify a published generation

```sh
cad verify out/bracket
cad verify out/bracket --manifest-hash "$TRUSTED_MANIFEST_HASH"
```

New build manifests carry an independent `manifest_schema_version` and an
`artifact_integrity` table with BLAKE3 digests and byte lengths for every payload,
including captured source, native geometry, exports and display assets. The
manifest excludes its own bytes from this table; `verify` returns their digest
as `manifest_hash`, which a release record can pin outside the generation.
The existing `schema_version` continues to describe the recipe schema.

Verification reads the published directory without evaluating a recipe or using
a geometry cache. It checks the complete file set, sizes and digests, rejecting
missing, modified or unexpected files, unsafe paths and nonregular entries.
`--manifest-hash` additionally checks the exact manifest against a digest you
already trust. Without that pin, replacing both a payload and its manifest can
produce another internally consistent generation. Neither mode certifies the
geometry, authenticates its author, or replaces engineering validation evidence.

Older manifests without the integrity table require rebuilding before they can
be verified. Verification failures produce structured JSON and leave the
generation untouched. Store verification reports outside the generation so they
do not become unlisted files.

The format bounds manifests to 16 MiB and generations to 100,000 payload files.
Payload verification streams file bytes rather than loading complete exports
into memory. Keep a generation immutable while verifying it; the command does
not lock files against other programs that overwrite them in place.

An empty Boolean result is valid for `inspect` and `check`, with zero volume and
null bounds. `build` reports an explicit export error because this STEP writer
requires a solid; no previous artifact generation is replaced.

### Collect engineering release evidence

From a clean Git checkout on Linux, with `cadquery-ocp` installed in a separate
Python environment:

```sh
python3 scripts/evidence.py collect --ledger evidence/ledger.json \
  --capability machining.counterbore-fixture --python /path/to/oracle/bin/python \
  --out-dir /tmp/counterbore-evidence
python3 scripts/evidence.py verify /tmp/counterbore-evidence --source-root .
CAD_EVIDENCE_BUNDLE=/tmp/counterbore-evidence python3 -m unittest discover -s scripts/tests -v
```

The collector builds the tested CLI, runs native and independent checks, and
retains source, binary, capability catalog, command logs, oracle versions and
generated models. The first case checks 27 native parameter combinations and
three independently imported STEP/STL configurations, retaining expected and
measured full moments. Each generation also passes `cad verify`. A successful
bundle validates only the named engineering case for its recorded revision;
the source ledger remains experimental until evaluated for another release.

The bundle uses SHA-256 separately from CAD generation BLAKE3 inventories.
Verification returns `manifest_sha256`; use an independently trusted value with
`--manifest-hash` on later verification. It executes no bundled programs.
`--source-root` additionally compares the clean Git revision and tracked source
bytes. Keep bundles immutable during verification. Checksums do not independently
prove command execution: trust the reviewed collector/CI and preserve its pin.

Collection executes trusted ledger argument arrays without a shell. It refuses
dirty source, incomplete evidence, planned capability promotion and existing
destinations. Failed attempts retain staging logs without replacing prior bundles.
Verification is portable; atomic no-replace collection currently uses Linux.
The separate engineering-evidence CI job collects this case and tests altered
evidence rejection. Hardware performance promotion, hermetic replay, signed
approvals and complete manufacturing-release gates remain future work.

## Rebuilds and reproducibility

Build a dependency closure for one or more outputs with repeatable `--output`:

```sh
cad build examples/workcell_line.star --output service_station --cache-dir .cad-cache --out-dir out/service_station
cad inspect examples/workcell_line.star --output service_station
cad watch examples/workcell_line.star --output service_station --out-dir out/service_station
```

`build`, `inspect`, and `watch` without this flag retain full evaluation.
`properties --output` and `bom --assembly` evaluate their requested output.
`check` always evaluates the full recipe. All modes validate the complete recipe's
structure and enforce **every** global face/property assertion and face-derived datum;
those obligations can require geometry outside the selected output. Unselected
geometry operations without such obligations are not executed. A selected
success therefore does not certify every other output's geometry.

Build/inspect reports expose `scope`: requested outputs, evaluated/skipped feature IDs,
and validation roots. Source and compiled recipe snapshots remain complete.
A selected build publishes a **complete generation of the selected outputs** in
its destination, replacing any previous owned generation there; it does not
merge into an existing full build. Use a separate destination to retain both.
Unknown output names and failed validation preserve the previous generation.

The workcell's `service_station` needs four unique solids even when the full
recipe describes 1,000 stations with 40 unique tool variants. Editing
`tool_rise_extra` for `edited_variant=1` leaves service-station geometry cached;
editing variant 0 rebuilds its single tool. The builder still parses and
structurally validates the full source, and retains evaluated intermediates and
in-memory cache entries. Selective execution is not yet a memory-bounded job
scheduler or an out-of-core exact-body store.

```sh
cad build examples/bracket.star --cache-dir .cad-cache --out-dir out/bracket
cad build examples/bracket.star --cache-dir .cad-cache --out-dir out/bracket
```

The second build reuses geometry from disk. Hashes include dimensions, dependency
hashes, numerical policy, schema/operation versions, kernel source/build identity,
and executor implementation identity. Logical IDs remain separate: identical
geometry can share data without sharing provenance. Cache entries are checked
for format, key, checksum, and geometric validity. Missing, stale, corrupt, or
inaccessible entries cause a rebuild. The cache is a local optimization, not a
trusted artifact distribution protocol.

Recipes, tests, `Cargo.lock`, and `rust-toolchain.toml` belong in Git. Build
artifacts and the local cache are ignored. No wall-clock timestamp is embedded
in exports. Fixed inputs/toolchain produce deterministic STEP/STL/B-rep output;
cross-platform bit-identical floating-point geometry is not promised. Manifests
contain cache statistics, so warm and cold builds can differ there.

No filesystem, network, clock, or module-loading API is exposed to recipes.
Starlark has stack, heap, tick, feature-count, and output-count limits. Those
limits are not an OS security sandbox. Running hostile recipes as a service
requires process isolation and resource limits outside this CLI.

## Current implementation architecture

The broader [target architecture](docs/architecture.md) separates the feature
graph, geometric topology, assembly instances, exact geometry, and streamed
display assets. The diagram below describes what runs today.

```text
Starlark / JSON / Rust Feature values
                  │
                  ▼
 cad-build: typed graph, validation, provenance, memory/disk cache
                  ├── cad-sketch: bounded 2D constraint solving and diagnostics
                  │
                  ▼
 cad-kernel: analytic geometry, oriented B-rep, bounded modeling operations
                  │
          ┌───────┴────────┐
          ▼                ▼
 cad-export             cad-preview
 STEP / STL             SVG / interactive HTML
          └───────┬────────┘
                  ▼
 cad-mesh: shared certified tessellation for one publication job
```

A separate `cad-assembly` crate validates shared part definitions and rigid
occurrences. It preserves logical identity, local geometry reuse, and placement
metadata independently of the feature DAG.

`cad-mesh` reuses analytic meshes and certified planar-face triangulation across
preview and STL writers. Each publication owns its cache; native geometry and the
complete numerical/meshing policy determine reuse. STEP and engineering queries
retain native geometry. Cache retention is bounded separately from construction
working memory, output strings and meshes still held by callers. The manifest
reports cache activity; this optimization does not relax native validation.

The kernel stores vertices, edges with supporting lines, oriented edge uses,
loops, faces with supporting planes, shells, and solids. Volume is integrated
from the oriented boundary, not copied from dimensions. Operations construct
new results without mutating inputs and validate before returning.

Orthogonal Booleans partition space at the **input coordinate planes**, classify
the resulting cells, and reconstruct their exposed boundary. This is not a fixed
resolution voxel approximation: boundaries retain input coordinates within
floating-point arithmetic. Coplanar faces may remain subdivided. The viewer hides
coplanar subdivision edges when drawing model edges.

The numerical policy rejects nonfinite dimensions, dimensions at/below tolerance,
unsupported subnormal integration, overflow, and translations that lose too much
precision. Distinct coordinate planes at/below tolerance separation are rejected
instead of silently snapped. Default tolerance is `1e-7` mm. Arrangement limits
bound memory and classification work.

History contains generated roles and explicit input-face descendants for
translations and orthogonal Booleans, including splits and deletions. Boolean
outputs also expose a `boundary` role. General persistent naming remains future
work. Public snapshots must be revalidated after external editing or
deserialization. STEP/STL exporters validate inputs.

## GPU assembly inspection

Normal assembly builds produce an additional `*.gpu.html` preview, linked from
`cad serve` alongside the portable Canvas viewer. This is our own WebGL2 renderer;
it uses no Three.js or other graphics library. Repeated occurrences share GPU
geometry buffers. Picking retains occurrence and local face identity, and the
inspector shows the captured recipe and named datums.

To inspect a larger assembly without generating engineering exports:

```sh
cad build examples/display_rack.star --display-only --param rows=100 --param columns=100 --param variants=1 --out-dir out/display_benchmark
cad serve out --port 8080
# Open http://localhost:8080/display_benchmark/display_rack.gpu.html
```

`rows × columns` controls occurrence count; `variants` controls unique geometry.
The default recipe contains 400 occurrences and four unique meshes. The command
above contains 10,000 occurrences and one mesh. This is an inspection workload,
not evidence of more advanced geometric modeling.

`--display-only` also works with `watch`. It preserves source snapshots,
parameters, geometry validation and atomic publication. Assembly outputs contain
embedded GPU HTML, streaming HTML, compact metadata and shared mesh assets;
ordinary solid outputs retain portable HTML.
No exact B-rep, STEP, STL, SVG, or separate BOM files are published in that mode.
Switching publication modes removes obsolete artifacts after a successful build.

Assembly builds also produce `*.stream.html` and `*.stream.json`. The streaming
viewer loads visible shared meshes separately, verifies their byte length and
SHA-256, and uses four concurrent requests plus a 64 MiB geometry accounting
budget. Bounds proxies keep unloaded or failed parts visible; selecting a proxy
prioritizes its mesh. The budget covers encoded payload plus geometry buffers,
including pending reservations, not total browser or driver memory. Frustum
culling uses an occurrence bounding-volume hierarchy in both GPU modes, skipping
off-screen groups before checking individual parts. Original occurrence and face
IDs remain stable. All occurrence metadata is still loaded at startup; metadata
paging, cluster streaming and occlusion culling remain future work. The streaming
viewer needs HTTP(S) and Web Crypto; localhost works.

Curved parts now have up to three shared display levels, built with native
tessellation policies of 0.5, 0.1 and 0.01 mm. The streamed viewer loads a coarse
surface first and refines toward a one-physical-pixel geometric detail target,
including a conservative numerical allowance. It keeps the current surface
while a replacement loads, uses intermediate detail when the budget permits,
and reports unmet detail when memory, unavailable assets or the finest level
limit refinement. Pixel coverage and depth quantization are outside that target.
Planar parts reuse one mesh. The embedded GPU and Canvas previews retain their
fixed detail. Exact geometry, STEP/STL and engineering checks do not depend on
the camera or selected display level.

`examples/spindle_line.star` combines tapered housings, shafts, retainer rings
and drilled bases in 12 reusable stations. Its six unique meshes serve 48 part
occurrences. Inspect the overview, then zoom into a housing or bore to see
progressive detail. Larger configurations use `--display-only`; station count
and housing variants independently vary reuse and unique geometry.

```sh
cad build examples/spindle_line.star --display-only --param stations=1000 --param columns=40 --param variants=12 --out-dir out/spindle_scale
CAD_BROWSER=/path/to/chromium node tools/preview-check/lod-benchmark.cjs
```

The benchmark expects `out` served on port 8080; `CAD_LOD_URL` overrides its URL.
It records cold coarse coverage and refinement into one station, verifies the
picked retainer face, and distinguishes software rendering from hardware gates.

`examples/workcell_line.star` demonstrates 12 stations, 60 occurrences and 15 part
definitions with datum-mounted triangular tools. Its independently expected
volume is 411147 mm³ and bounds are [0,0,0]–[700,360,42] mm. Build it normally and
open `/workcell_line/workcell_line.stream.html` from the gallery. The selective
loading harness is `tools/preview-check/stream-benchmark.cjs`.

The GPU viewers require WebGL2 and a manual reload after a rebuild. They use local
and camera-relative float32 coordinates and report explicit
errors outside its display range. It is an approximate inspection client, never
an engineering measurement or modeling backend. Normal builds include a portable
preview fallback; display-only assemblies need a WebGL2-capable browser.

GPU publication permits 100,000 occurrences, 50,000 unique faces and 200,000
unique face points. These are resource limits, not frame-rate claims. The
benchmark command records the actual renderer and environment:

```sh
CAD_BROWSER=/path/to/chromium node tools/preview-check/gpu-benchmark.cjs
# Defaults to SwiftShader for reproducible software-rendered CI.
# Set CAD_GPU_HARDWARE=1 to request the available hardware path; inspect the report.
```

## Supported domain and current limits

- Rectangle profiles and simple CCW polygon outer rings with optional convex CW
  holes; 256 total vertices and at most 32 disjoint interior holes.
- Native positive-Z extrusion with authoritative polygonal caps and planar walls.
- Translation of supported solids; rigid exact transforms within the domain above.
  Unions, cuts, intersections, open pockets,
  through-slots, and separated multiple solids in the orthogonal domain.
- Two parallel solid cylinders support analytic intersection over a common axial
  interval, union with matching intervals, and through-cuts. Contact and uncertain
  separation fail explicitly; arbitrary curved operands and chained lens/crescent
  results are outside this two-cylinder branch. A separate drilling path supports
  contained, disjoint circular through-cuts in recognized polygon and line/arc
  prisms, including earlier drilled parts and supported cylinder-Boolean results.
- General planar validation supports one strictly convex solid with supporting
  outward face planes, or one matched-cap right prism with a simple outer ring and simple holes.
  Multiple general solids are explicitly
  unsupported in one kernel snapshot; assemblies retain separate definitions.
- Closed orientable shells, including through-hole topology.
- Explicit rejection of enclosed cavities, nonmanifold contacts, unsupported
  geometry, and computations outside the numerical/resource envelope.

Round-hole cutting in arbitrary solids, general rotated nonconvex modeling, general sketch constraints, general
polygon union/cut and nonconvex polygon Booleans, general trimmed curved surfaces,
fillets, offsets, NURBS, STEP import, and GPU
modeling are **not implemented**. STEP uses AP214 `FACETED_BREP` with planar
boundaries and millimetre units, with occurrence relationships for assemblies; it does not preserve the executable recipe.
Analytic cylinders use `ADVANCED_BREP_SHAPE_REPRESENTATION` with circles, a cylindrical surface and an explicit periodic seam. STL is unitless by specification, with coordinates written in millimetres.

The orthogonal baseline and first convex planar extension are bounded increments
toward the commercial CAD goal. Nested assemblies, scoped face references and
shared display assets are implemented.
Named part datums and part/rigid-module attachments are implemented. Next come
reference-driven modeling, bounded-error display LOD,
broader analytic geometry, general planar Booleans, and general trim/seam correctness.
The [roadmap](docs/roadmap.md) defines release evidence
for curved modeling, constraints, large assemblies, and manufacturing workflows.

## Analytic circular parts

`circle(id = "profile", radius = 8)` followed by `extrude(..., height = 60)`
creates an authoritative cylinder with three faces, three edges and two seam
vertices. Native transforms, mass properties, cap datums, STEP and inspection
preserve its circular geometry. See [shaft_stack.star](examples/shaft_stack.star)
for a shaft, flange and end cap attached through face datums, plus a separately
rotated shaft. These are separate solid parts; general curved Booleans remain unsupported.

```sh
cad build examples/shaft_stack.star --out-dir out/shaft_stack
cad properties examples/shaft_stack.star --output shaft
```

Set `inner_radius` to create a true annular profile, for example
`circle(id = "bushing_profile", radius = 10, inner_radius = 6)`. Zero (the default)
creates a disk; a positive bore and the remaining wall must exceed the recipe's
linear tolerance. Extrusion retains annular caps and an inward cylindrical face
with its own periodic seam. `inner_wall` identifies that face in recipe selectors;
`wall` remains the outer wall. Both cap datums use the material area centroid,
which lies in the bore for a concentric annulus.

[bearing_cartridge.star](examples/bearing_cartridge.star) combines a bronze sleeve,
two shared steel retainer rings and a separate shaft with prescribed radial
clearance. Its bore and length parameters drive geometry, cap attachments, BOM,
and mass properties. `npm run test:tube --prefix tools/preview-check` verifies
through-bore visibility and native face picking against independent ray/cylinder
intersections. `cad-export`'s `export_tubes` example produces straight and oblique
STEP fixtures for the independent OCCT tools.

Display/STL tessellation uses a default 0.01 mm radial chord-error ceiling and
at most 4,096 circumference segments; an unmet error budget fails explicitly.
The Rust tessellation API accepts a policy. Display assets record the achieved
bound and map every wall patch to the same native face. Viewer measurements
remain approximate. STEP keeps the analytic boundary independently of this mesh.

Kernel 0.3 changes native JSON from `edge.line`/`face.plane` to tagged
`edge.curve`/`face.surface`. Rebuild old generated artifacts from recipes; old
native snapshots fail deserialization instead of being silently reinterpreted.
Existing planar recipes remain supported. Display assets with semantic curved
patches use envelope version 3; older stream clients require a reload.

### Circular holes in polygon plates

Polygon profiles can combine polygonal relief slots with true circular bores:

```python
profile = polygon(id = "mount_profile",
    points = [[0,0], [80,0], [80,50], [0,50]],
    circular_holes = [{"center": [40,25], "radius": 10}])
plate = extrude(id = "mount", profile = profile, height = 8)
output(name = "plate", feature = plate)
```

Each bore retains circular cap boundaries and an inward cylindrical wall in
native geometry and STEP. `circular_hole_0_wall` selects the first bore wall;
hole order is part of the source's reference intent. Holes must be strictly
inside the material and separated from every other boundary by the numerical
policy. The combined polygonal/circular hole limit is 32. Display/STL caps allow
at most 2,048 sampled boundary points; an excessive tessellation request fails
explicitly. This is profile extrusion, not a general curved Boolean cut.

[drilled_bearing_mount.star](examples/drilled_bearing_mount.star) combines a
chamfered plate, two relief slots, five bores, a bronze sleeve, a shaft, and four
reused bolt assemblies. Parameters change plate thickness, bearing bore and bolt
radius. The sleeve and shaft attach to the remaining cap's material centroid.
Prescribed clearances are design intent, not a certified interference result.

### Dimension-driven sketches

`sketch(id, points, constraints, outline=None, holes=[], circular_holes=[], curve_outline=None)` records design intent for
the headless builder. Each point has an `id` and a two-coordinate `position`
initial guess. Each constraint has a unique `id` and a `kind`: `fixed` uses
`point` and `position`; `coincident`, `horizontal` and `vertical` use `a` and `b`;
`distance` adds a positive `value` in millimetres. Four segment constraints use
endpoint-ID pairs such as `a = ["a", "b"]`, `b = ["b", "c"]`: `equal_length`,
`parallel`, `perpendicular` and `angle`. Directed `angle` adds finite `degrees`
in (-180, 180], measured from the first segment to the second; parallel and
perpendicular relations are unoriented, modulo 180 degrees. `circle_line_tangent`
uses `circle`, `line` and an explicit `side`, as described below. `outline` lists named vertices
in CCW order, and optional hole rings use CW order. Rings omit the closing point.
The solved profile feeds `extrude`; sketch-driven revolution is not yet supported.

The original Rust `cad-sketch` service uses analytic Jacobians and a bounded
local nonlinear solve. It reports constraint residuals, local rank and degrees
of freedom, redundant equation count, conditioning and implementation identity.
Schema-3 reports separate `max_residual_mm` and
`max_angular_residual_radians`; each constraint emits exactly one of
`residual_mm` or `residual_radians`. Linear constraints use the requested
`tolerance_mm`; angular constraints use the independent
`angular_tolerance_radians = 1e-9`. Fixed/coincident residuals are Euclidean
distances. The conditioning bound applies to the mixed normalized-linear/radian
Jacobian, not to coordinate error. Initial guesses select a
local branch; convergence does not prove a unique solution or continuity across
edits. There is no automatic geometry repair or arbitrary placement of free
points. Zero or numerically unresolved segment directions fail explicitly.
An angular residual within 1e-8 radians of its wrap cut is unresolved; trial
steps that can cross that cut are damped rather than silently changing branches.
Underconstrained, unconverged or numerically unresolved results cannot
publish geometry; the previous generation remains available.

Sketches support up to 256 points and 1,024 constraints, partitioned by shared
point references into independent constraint islands. Each island retains the
32-point, 128-constraint and 256-iteration limits; there are at most 128 demanded
sketches per build. All island sizes are checked before numerical work. A large
connected component still fails explicitly; this is not yet a general sparse
solver for large connected sketches. All definitions are structurally checked;
selected builds solve their dependency closure and global validation obligations.
Cached solids do not bypass solving or discard diagnostics. Build/check manifests
contain `sketch_reports`, keyed by feature ID and paired with `feature_hashes`;
failure JSON includes the affected feature, input hash, solver identity and report.
These are numerical results under the declared tolerance, not symbolic coordinates.

Schema-3 `islands` records expose sorted point/constraint IDs, status, residual
maxima, rank, degrees of freedom, iteration count and conditioning for each group.
Each group has its own numerical origin and scale. Overall rank, degrees of
freedom, redundant equations and snapshot iterations are sums; residual maxima
are maxima. `condition_bound` is the worst local bound, present only when every
island has one; it is not the condition number of a globally scaled Jacobian.
A failed island blocks geometry publication while reports retain healthy groups.
The solver fingerprint invalidates prior geometry cache entries. Recipe schema
remains 2; old solve-report readers must explicitly upgrade to schema 3.

[modular_bore_plate.star](examples/modular_bore_plate.star) exercises 30 circular
bores in a 180×150 mm plate, with 64 points in 34 islands. Thirty annular guide
instances share one definition; the plate and two feet complete 33 occurrences
using three definitions. Parameters are `pitch` (22..28 mm), `bore_radius`
(3..5 mm) and `thickness` (6..10 mm). Independent array formulas check 27 designs
for volume, centroid, full inertia and material mass. Pitch edits rebuild only
the plate and move the guide occurrences; radius edits rebuild two definitions.
Three independent STEP cases verify moments and assembly hierarchy. Run
`npm run test:islands-sketch --prefix tools/preview-check` for nine browser cases
and 2,160 bore/guide/plate visibility checks. One local full release publication
measured 3.36 s and 46,484 KiB peak process RSS; this is a single shared-host
observation, not a performance guarantee. Its scope, binary/source identities
and environment are in [the measurement record](docs/benchmarks/islands-modular-bore-local.json).

[constrained_slope_fixture.star](examples/constrained_slope_fixture.star) solves
a triangle from three side lengths, then seats a deck and two shared rails on
its inclined face. A rounded mounting base and four bolts complete nine
occurrences using five definitions. The recipe contains starting guesses rather
than a formula for the triangle apex. Independent circle-intersection formulas
check 27 dimension combinations, support inertia, mass, bounds and deck placement.
Changing one side length rebuilds the support extrusion and transform while
reusing the deck, rails, base and bolts. The viewer remains an inspection client;
changing dimensions requires a recipe build.

[constrained_angle_fixture.star](examples/constrained_angle_fixture.star) solves
an inclined triangular support using an angle and perpendicularity, plus an
equal-leg trapezoid deck using parallel edges. Its deck and two shared side pads
follow solved face datums; a rounded base and four bolts complete nine occurrences
using five definitions. Parameters are `span` (50..80 mm), `tilt_degrees`
(15..45 degrees) and `deck_top_width` (18..30 mm), defaulting to 60, 30 and 24.
Independent trigonometry gives support height `span*tan(tilt_degrees)` and deck
height `sqrt(44²-((36-deck_top_width)/2)²)`. Native checks pass across 27 configurations; three configurations also pass
independent STEP readback and all three browser clients (nine browser cases).
Run `npm run test:angular-sketch --prefix tools/preview-check` for browser
acceptance and `crates/cad-export/tools/check_angle_fixture_ocp.py` with the
optional OCP environment for exchange acceptance. Independent arc variables, circle-to-circle tangency, branch-continuation policies and assembly
joints remain future work.

### Constrained circular holes

`sketch(..., circular_holes=[{"center":"bore", "rim":"radius_handle"}])`
uses solved point identifiers to define a circular through-hole. Its centre is
`bore`; its radius is the solved distance to `radius_handle`. Add a distance
constraint for the radius and, for example, a horizontal constraint to remove
the handle's rotational freedom. Every construction point must be fully
constrained. Initial guesses still select a local solution branch.

These circles remain analytic through native extrusion and STEP. Polygonal and
circular holes share the 32-hole limit; unknown handles, zero radii, touching,
intersecting or outside holes fail validation before publication. This does not
yet introduce independent circle variables, circular outer boundaries or independent arc variables.
Circle-to-line tangency is available through the point-handle contract below.

[constrained_drill_jig.star](examples/constrained_drill_jig.star) combines a
three-hole plate, three shared annular guide bushings and two shared feet.
Parameters are `pitch` (20..30 mm), `bore_radius` (3..5 mm), and `thickness`
(6..10 mm). Hole spacing and radius come from sketch constraints. Guide
placements reuse those declared dimensions as prescribed transforms; they are
not associative circular-face attachments or a fit certification. Native tests
cover 27 configurations using independent circle-subtraction formulas for
volume, centroid and inertia, plus material rollups. Pitch edits rebuild only
the plate; radius edits rebuild the plate and shared guide definition.
Three independent STEP configurations check geometry and assembly hierarchy;
`npm run test:drill-sketch --prefix tools/preview-check` checks 216 bore/ring/plate
rays across nine browser cases. Invalid solved profiles return
`invalid_polygon_profile`, the feature ID and a diagnostic, preserving the
previous complete generation. Readers that do not support the new optional
sketch field reject it explicitly.

### Circle-to-line tangency

```python
{"id":"seat", "kind":"circle_line_tangent",
 "circle":["center","radius_handle"], "line":["start","end"], "side":"left"}
```

This dimensional constraint makes the circle tangent to the **infinite supporting
line** directed from `start` to `end`. `left` means positive signed distance in
the sketch XY plane; `right` means negative. The circle's radius is the distance
from its centre to its radius handle. Contact may lie beyond the endpoint
segment. Reverse the line and swap the side to preserve the same intent.
The constraint does not fix radius or handle direction; author the necessary
additional dimensions. Degenerate directions and insufficient numerical
precision fail explicitly. Residuals are millimetres, not radians.

[tangent_bore_fixture.star](examples/tangent_bore_fixture.star) locates two bores
inside opposed construction wedges without calculating their centres in the
recipe. The plate, two feet and four clamps form seven occurrences sharing three
definitions. Parameters are `half_width` (20..35 mm), `bore_radius` (3..6 mm),
and `thickness` (6..10 mm). Independent wedge formulas verify 27 configurations;
three STEP cases check moments and hierarchy. Nine browser cases check 144
bore/plate/clamp rays and captured source with `npm run test:tangent-sketch
--prefix tools/preview-check`. Construction tangency does not certify physical
contact, holding force, finite-segment contact or assembly constraints.

### Curved outlines from solved points

Use `curve_outline` instead of `outline` to describe an outer line/arc path by
point identifiers. Each `line_to` names its end point; each `arc_to` names its
centre and end point. The final end identifier must equal `start` explicitly.

```python
curve_outline = {"start":"a", "segments":[
    {"kind":"line_to", "end":"b"},
    {"kind":"arc_to", "center":"right_center", "end":"c"},
    {"kind":"line_to", "end":"d"},
    {"kind":"arc_to", "center":"left_center", "end":"a"},
]}
```

All referenced points come from the fully constrained sketch. Curved profiles
support simple CCW outlines, including concave recesses and clockwise arcs of at
most a half turn, with circular through-holes. Polygonal holes in curved paths,
major arcs and sketch-driven revolutions remain unsupported.
Curved profiles use bounded residual refinement after initial sketch convergence,
without changing the declared acceptance tolerance or projecting coordinates.
If refinement stagnates, the best fully certified snapshot is returned with its
own iteration number. The builder then validates the native path before using
cached solid geometry; even a converged sketch can fail this stricter geometry
gate. Ordinary polygon sketches keep their original solve policy.

[constrained_capsule_link.star](examples/constrained_capsule_link.star) uses a
solved capsule outline and two bores in two shared link plates, with two shared
spacers and two stepped pins. The six occurrences share three definitions.
Parameters are `span` (40..80 mm), `outer_radius` (8..14 mm) and `bore_radius`
(3..5 mm). Initial guesses are retained in the recipe. Placements and pin
clearances are prescribed design intent, not mechanism or contact certification.
Independent rectangle/semicircle integrals verify 27 configurations, full link
inertia and assembly mass. Span edits rebuild one definition, outer-radius edits
two and bore-radius edits three. Asymmetrically perturbed initial guesses are
also tested; native geometry checks remain mandatory. Three independent STEP
cases verify moments and assembly hierarchy; `npm run test:capsule-sketch
--prefix tools/preview-check` checks nine browser cases and 126 visibility rays,
including arc silhouettes, bores, pins and captured source.

### Native line and arc profiles

`path(id, start, segments, circular_holes=[])` defines a closed simple CCW
profile for `extrude`, including concave outlines. Each segment is a `line_to`
with an `end`, or an `arc_to` with a `center` and `end`; `clockwise` defaults to
false. Arcs may turn either direction but must be nonzero and at most π radians.
Clockwise recesses produce native inward cylindrical walls. The final endpoint must
exactly equal `start`. Tangent line/arc joins are supported; unsupported winding,
major arcs, mismatched radii, open paths and unresolved numerical inputs fail
explicitly. Limits are 256 segments and 32 circular holes with strict separation.

```python
slot = path(id = "slot_profile", start = [-10,-5], segments = [
    {"kind":"line_to", "end":[10,-5]},
    {"kind":"arc_to", "center":[10,0], "end":[10,5]},
    {"kind":"line_to", "end":[-10,5]},
    {"kind":"arc_to", "center":[-10,0], "end":[-10,-5]},
])
body = extrude(id = "slot_body", profile = slot, height = 8)
```

Trimmed circular edges and cylindrical walls remain analytic in the native model
and STEP. Signed analytic boundary integrals supply volume, centroid, full
inertia and cap datums. `segment_i` names each source wall; `start_cap`, `end_cap`,
`walls` and `circular_hole_i_wall` preserve their normal meanings. Line-only
paths lower to the existing polygon-prism implementation with segment aliases.
Display meshes share arc samples, include endpoint arithmetic in their chord-error
budget and independently certify cap coverage. A requested coarse mesh that
closes a native clearance fails explicitly; reducing chord error can resolve it.
This does not add general curved Booleans, curved inner paths or partial revolution.

[rounded_clevis.star](examples/rounded_clevis.star) combines a rounded base,
two shared arched cheeks, a capsule link and a pin: five occurrences from four
part definitions. Independent formulas check material volumes, link centroid and
full inertia, assembly mass, bounds and datum placements across 27 parameter
combinations. Editing `link_length` rebuilds one definition, `clevis_gap` two,
and `corner_radius` one. Native property assertions also run for selected-body
builds. The pose is prescribed by datums, without a joint or load-solver claim.
Independent STEP checks cover hierarchy, placements and native properties.
Browser acceptance checks capsule and rounded-rectangle caps, curved walls and
bores against analytic rays in Canvas, GPU and streamed views, including oblique
instances; source inspection fetches no exact geometry.

### Revolved stepped and tapered parts

`revolve(id, profile)` turns a polygon's `[radius, height]` coordinates through
360 degrees about +Z. The closed profile must be counterclockwise and stay at nonnegative radii.
Horizontal edges create planar shoulders, vertical edges create cylinders, and
sloped edges create conical frusta or pointed conical ends. It may touch the axis
along one interval or remain entirely away from it for an annular part. Up to
256 vertices are supported. Sloped edges require radial/axial spans above
tolerance; an endpoint may meet the axis at a true pole. Separate profile holes,
isolated axis contacts, pinched shells and partial turns are currently rejected.

```python
profile = polygon(id = "shaft_profile",
    points = [[0,0], [8,0], [8,20], [15,20], [15,26], [6,26], [6,50], [0,50]])
shaft = revolve(id = "shaft", profile = profile)
output(name = "shaft", feature = shaft)
```

This produces one connected solid with true cylindrical/conical walls, circular
edges and planar shoulders. `segment_i` identifies the face generated by source edge
`i`; an edge on the axis generates no face. Native rigid transforms, analytic
mass properties, planar face datums and STEP preserve the curved geometry.
Display/STL share circle samples across adjacent faces and enforce the requested
chord error, with a 200,000-patch resource limit for revolved meshes. A pole is
a real shared vertex, with no zero-radius circle or artificial cap; its display
patches are triangles. A pointed upper end has no `end_cap` face role; a pointed
lower end has no `start_cap` face role.

[stepped_spindle.star](examples/stepped_spindle.star) places a stepped shaft,
counterbored housing and blind-bored cap in an exploded inspection layout. The
shaft's length and flange radius are editable parameters. These are connected
turned parts, but their separate display placements do not constitute a solved
mechanism or a verified bearing fit.

[tapered_nozzle.star](examples/tapered_nozzle.star) adds outward chamfers, a
long taper, an inward countersink and a separate tapered pin. Its `neck_length`
parameter changes the taper while preserving source segment roles. Native cones
retain their reference radius and signed radial slope. STEP exports equivalent
positive-angle supports, reversing the axis for narrowing cones for importer
compatibility. This example makes no fluid-flow or manufacturing-fit claim.

[centering_fixture.star](examples/centering_fixture.star) seats two shared steel
center pins in opposite pointed blind bores of an aluminum workpiece. Datum
attachments place the pins, drilled holders and workpiece on a base. Changing
`tip_depth` rebuilds the pin and workpiece while preserving the assembly's total
material volume; `work_length` and `bore_clearance` are also recipe parameters.
Native volume, mass and balance assertions gate the build. This inspection fixture has six occurrences
and four part definitions, not a solved contact model or
certified fit. Solid cones, double-tip solids and inward pointed bores retain
analytic geometry through STEP export and rigid placement.

## Verify and package

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
./scripts/package.sh
```

Tests cover analytical measurements, parameter sweeps, topology corruption,
numeric boundaries, Boolean identities, pockets/through-holes, disconnected and
empty results, tolerance/resource rejection, graph errors, cache invalidation,
parameters, export orientation/volume, and complete CLI workflows. Acceptance
fixtures exercise the shipped bracket, tray, fixture, enclosure, intersection,
and repeated-instance recipes.

Independent STEP checks use the optional script in
`crates/cad-export/tools/check_step_ocp.py`. Install `cadquery-ocp` in a disposable
Python environment to run it. `check_common_ocp.py` also recomputes intersection
from two operand STEP files and compares both directed difference volumes:

```sh
cargo run --locked -p cad-export --example export_intersection -- /tmp/cad-common
python crates/cad-export/tools/check_common_ocp.py /tmp/cad-common/left.step /tmp/cad-common/right.step /tmp/cad-common/common.step
```

Append `skew` to the exporter example command to exercise two independently
rotated cubes, then run the same oracle on that directory.

OCP is never a runtime dependency. CI runs format,
lint, tests, and example builds, then uploads the generated artifacts.

The optional browser smoke test verifies view switching, face selection, edge
toggle, JavaScript errors, and mobile overflow:

```sh
npm ci --prefix tools/preview-check
cd tools/preview-check
npx playwright install chromium
npm test
npm run test:hosted
npm run test:refresh
npm run test:depth
npm run test:assembly
npm run test:nested
npm run test:polygon
npm run test:transform
npm run test:clipped
npm run test:datums
npm run test:gpu
npm run test:stream
npm run test:bvh:numeric
npm run test:bvh
npm run test:revolve
npm run test:apex
npm run test:serpentine
npm run test:arc
npm run test:sketch
npm run test:counts
npm run test:cone
```

Build the bracket first and run `cargo build -p cad-cli` before the hosted test.
For `test:clipped`, build `examples/clipped_tools.star` into `out/clipped_tools`.
For `test:transform`, build `examples/tilted_jig.star` into `out/tilted_jig`.
For `test:polygon`, build `examples/angled_fixture.star` into `out/angled_fixture`
using the command above; the test checks the triangular ramp silhouette and
picking against an independent XY half-plane oracle, including empty corners
of its bounding rectangle. Set `CAD_POLYGON_PREVIEW` to override that preview path.
For `test:datums`, build `examples/datum_gantry.star` into `out/datum_gantry`.
It checks 14 occurrences sharing five meshes, analytically expected rotated tool
vertices, and the selected part’s datum metadata and exact recipe panel.
The hosted test starts temporary server/watch processes and checks live recipe
edits, automatic refresh, failed-build preservation, recovery, and STEP download.
The refresh regression simulates interleaved generations during edit/revert so
a manifest and preview from different builds cannot be combined.
Set `CAD_BIN` to test a different built executable. Set `CAD_BROWSER` to an
existing Chromium executable instead of installing a Playwright browser if
preferred. The depth test checks rendered visibility and picking against
independent ray/rectangle intersections. Node is only a test dependency; exported
viewers have no external dependencies.

### Split bearing block with concave seats

[split_bearing_block.star](examples/split_bearing_block.star) uses a clockwise
semicircular recess in each of two shared jaws, a stepped shaft and four shared
annular sleeves: seven occurrences from three definitions. `seat_radius`
(10..18 mm), `depth` (12..24 mm) and `jaw_gap` (0..3 mm) expose geometry and
placement edits; changing the gap rebuilds no solids, radius rebuilds two
definitions and depth rebuilds three. The `jaw` output exposes the native seat
without the shaft covering it.

Independent rectangle-minus-semicircle integrals check volume, centroid and full
inertia over 27 configurations; assembly checks include density-weighted mass
and inertia. Invalid crossing revisions preserve the last published generation.
Analytic segment intersection, boundary distance and ray winding validate native
profiles; tessellation is not used to decide whether a path is simple or a hole
is inside. Nonadjacent contact, overlap, uncertain separation and cusps fail
explicitly. Placements remain prescribed: this example does not certify fits,
contact forces, bearing life or constrained motion.

Three independent STEP-import cases additionally verify trimmed inward cylinder
orientation, assembly placement and closed STL output. `npm run
test:concave-profile --prefix tools/preview-check` checks 12 browser views and
706 independent visibility rays, including isolated jaw recesses and captured
source inspection.

### Twin-lobe fixture with native curved Booleans

[twin_lobe_fixture.star](examples/twin_lobe_fixture.star) unions two circular
blanks into a base and partitions a second layer into two crescents and a lens.
Two shared annular bosses complete six occurrences from five definitions.
`left_radius` (20..24 mm), `spacing` (26..34 mm) and `depth` (6..10 mm) drive
geometry edits. Isolated `left_lobe`, `overlap` and `union_base` outputs expose
the trimmed boundaries; picking a part shows the captured Starlark source.

The original Rust kernel intersects circles analytically and constructs native
trimmed cylinder faces. Source-wall references survive splitting, including the
inward wall of a cut. Independent circular-segment integrals check volume,
centroid and full inertia over 27 configurations, plus assembly mass and cache
invalidation. Tangent contact, partial-depth cuts and unresolved cap offsets
reject the revision and preserve all previously published artifacts.

This branch accepts two recognized solid cylinders with parallel or opposite
axes. Union requires matching axial intervals; intersection uses their common
interval; a cutting cylinder must cover the left operand axially. Annular
operands, arbitrary curved bodies, disconnected curved unions and general
further Booleans on the resulting lens/crescent are unsupported. The contained
through-bore path below can drill recognized arc-prism results. Modeling never uses
the display mesh. This fixture is a prescribed layout, not a fit/contact proof.

Independent STEP acceptance covers three recipe configurations plus six direct
Boolean cases, including operations on obliquely transformed operands.
`npm run test:circular-boolean --prefix tools/preview-check` checks nine browser
views and 2,169 visibility samples across Canvas, embedded GPU and streaming GPU
clients, with exact captured source and no exact-geometry downloads.

### Machined bearing support with sequential drilling

[machined_bearing_support.star](examples/machined_bearing_support.star) starts
with a solid jaw containing a concave seat, then uses four successive `cut`
features to drill mounting bores. The assembly reuses that finished jaw twice
with a stepped shaft and eight shared sleeves: eleven occurrences, three
definitions. Inspect `stock` beside `drilled_jaw` to see the machining result.

The bore path preserves existing native faces and their source references, adds
true circular cap boundaries and inward cylindrical walls, and retains each
cutting tool's wall provenance. `seat_radius` (14..22 mm), `bore_radius`
(2..4 mm), and `depth` (12..24 mm) control the design. Independent signed
rectangle/semicircle/disk integrals verify full volume and material inertia over
27 configurations. Seat edits compute six solids, bore edits ten, depth edits
twelve; unaffected stock/tools/part definitions remain reusable as applicable.

Drilling requires a recognized right prism and a solid parallel cylinder covering
its full depth. The bore must be wholly within material and separated from
existing holes. Crossing, tangent, overlapping and outside bores fail explicitly;
cuts wholly inside an existing hole are also unsupported. Flat-bottom blind
bores use the additional native path below.
General pockets and intersecting drill passages remain future work. Failed
machining obligations preserve the prior published generation even when only
the unchanged stock is requested.

Independent STEP checks cover three support configurations and four direct
rectangle/capsule drilling cases with identity and oblique operand frames.
Imported drilled jaws retain five inward cylindrical faces and four periodic
seams. `npm run test:through-bore --prefix tools/preview-check` verifies twelve
browser views and 1,190 visibility samples, including exposed jaw holes, sleeve
bores and the exact captured recipe.

### Tooling pallets with blind sockets

[blind_bore.star](examples/blind_bore.star) isolates a flat-bottom cylindrical
cut. [blind_bore_pallet.star](examples/blind_bore_pallet.star) combines six top
blind sockets, two underside sockets and four through-bores in one native plate.
Two nested stations reuse that plate with stepped locating pins, feet and sleeves:
26 occurrences from four definitions. `pallet_inspection` exposes the machined
plate without inserted hardware; the pair shows the assembled result.

`socket_radius` (6..10 mm), `socket_depth` (8..16 mm), and `plate_height`
(24..32 mm) drive the recipe. Blind cuts retain a native planar floor and an
unopened outer cap. The stock's faces keep their references; the tool wall and
terminating cap contribute to `bore_wall` and `bore_floor`. Pins attach to
semantic floor datums, so a socket-depth edit moves their seats from geometry.
Full signed box/cylinder integrals independently verify volume, centroid, inertia and material
mass over 27 configurations. Depth edits reuse the unchanged stock, feet and
sleeves. Removing a required floor or introducing invalid machining prevents
publication, including requests for only the original stock.

This path supports parallel or opposite-axis solid cylinders in recognized
polygon or line/arc right prisms, including separated prior blind/through cuts.
The circular projections must be mutually clear, even when two opposite-side
pockets would not meet in depth. Positive depth and remaining floor thickness
must be numerically resolved. The additional counterbore path below handles
coaxial overlaps. Fully enclosed cavities, noncoaxial overlapping holes, skew
cuts, drill-tip cones and general pockets remain unsupported.

Independent STEP/STL checks cover identity and oblique direct cases plus three
pallet configurations. `npm run test:blind-bore --prefix tools/preview-check`
checks top and underside floor/cap picking and through-hole visibility across
Canvas, embedded GPU and streaming GPU clients, plus nested selection and exact
captured source. These prescribed layouts do not certify fits or load capacity.


### Counterbored fastening fixture

[counterbore.star](examples/counterbore.star) drills an annular seat around a
through-hole. [counterbored_fixture.star](examples/counterbored_fixture.star)
performs eighteen successive cuts to form six holes with counterbores on both
sides of a plate. Two mirrored plates, twelve washers, six spacer sleeves, six
headed fastener envelopes and six nut envelopes form a 32-occurrence assembly
from five definitions. The hardware attaches to semantic annular shoulder datums.
Changing counterbore depth moves the seats and attached hardware; unchanged
washers, nuts, spacers and pilot-drilled stock remain reusable.

`counterbore_radius` (6..8 mm), `counterbore_depth` (4..8 mm), and `plate_height`
(18..24 mm) drive the example. Inspect `plate_inspection` to see exposed shoulders
and central openings, or the assembled `counterbored_fixture`. Picking a part
shows the exact captured Starlark. Fasteners are analytic envelopes; threads,
fits and preload are not simulated.

The kernel takes the union of coaxial drilling intervals as a sequence of
nonoverlapping cylindrical sections. It supports drilling a small through-hole
after a large blind cut, enlarging either end of an existing hole, and successive
radial steps. Native shoulders are annular planar faces with shared circular
boundaries. Semantic history maps surviving portions of shortened walls and
shoulders, with explicit deletion when a feature is removed. Internal face
indices may change when the hole boundary is rebuilt.

Independent signed-slice integrals check full native and assembly moments over
27 configurations. STEP re-import and closed-STL checks cover both cut orders,
opposed counterbores, common oblique frames and three fixture configurations.
`npm run test:counterbore --prefix tools/preview-check` checks annular shoulder
picking and open pilot holes from both sides in all three viewers. Invalid cuts
or removed required shoulders leave the previous published generation intact.
Noncoaxial intersecting passages, conical countersinks and general pockets remain
future work.

### Machined round bearing cartridge

[round_stock.star](examples/round_stock.star) starts a blind hole directly in
annular stock. [round_bearing_cartridge.star](examples/round_bearing_cartridge.star)
combines two machined annular flanges, bearings, a common shaft, three socketed
spacer rods and twelve fastener/washer pairs: 32 occurrences sharing six part
definitions. Six counterbores and three blind sockets in each flange retain
semantic seats; fasteners and rods attach to those seats.

`bolt_pitch` (40..44 mm), `seat_depth` (4..8 mm), and `flange_height` (18..24 mm)
control the recipe. `flange_inspection` exposes the center bore, pilot holes,
shoulders and closed socket floors. The assembled `round_bearing_cartridge`
shows the mirrored pair. Threads, fits, loads and bearing performance are not
certified by these prescribed analytic envelopes.

The kernel splits the stock's outer periodic cylinder into two exact
semicylinder faces while retaining its native radius, axis and cap supports.
Original outer-wall lineage therefore resolves to two descendants; an untouched
central bore retains its own identity. This enables the existing contained
through/blind/counterbore operations without polygonizing the stock. Boundary
contact, cuts crossing the existing bore, skew axes and general intersecting
passages remain outside this machining domain.

Independent cylinder integrals cover 27 configurations, full native/material
moments and parameter-driven attachments. Twelve direct identity/oblique exchange
cases and three assembled STEP/STL oracle configurations cover the new boundary.
`npm run test:round-stock --prefix tools/preview-check` checks circular silhouettes,
through openings, socket floors and shoulder picking in all three viewers. The
ledger capability `machining.round-bearing-cartridge` collects its native and
retained independent product evidence using the release-evidence workflow above.
