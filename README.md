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
outlines and convex polygonal through-holes, plus true circular extrusions and stepped/tapered revolutions**: slotted brackets, open trays, triangular ramps, and polygonal
seating blocks. Convex solids also support native rigid transforms. Reusable
assemblies can place these parts with rigid rotations;
Union and cut remain orthogonal-only; intersection also supports bounded,
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
| `polygon(id, points, holes=[])` | Simple CCW outer XY ring (convex or nonconvex) with optional convex CW holes; 256 total vertices, at most 32 holes |
| `extrude(id, profile, height)` | Extrude a rectangle or supported polygon profile along positive Z |
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
Optional `holes` are convex **clockwise** rings strictly inside the outer
boundary. Holes must be disjoint, nonnested and separated by more than the
numerical/tolerance envelope; touching or uncertain arrangements fail explicitly.
There are at most 32 holes and 256 total profile vertices. Caps retain their
inner loops in the exact B-rep and STEP; derived STL/display triangles leave
openings empty. Self-crossings, nonadjacent edges within the numerical/tolerance envelope,
and holes that cross a concave bay are rejected. Inner hole rings remain convex.

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
preserves those roles through lineage. Polygon Boolean lineage is unsupported.

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

`cad init` never overwrites an existing recipe. Builds stage all artifacts before
publication. Linux replacement uses atomic directory exchange. Other platforms
use a rename/rollback fallback with a brief publication gap. Only empty
directories or directories containing exclusively artifacts listed in their CAD
manifest may be replaced. Unmanaged files are preserved by refusing replacement.
Output names must be safe, noncolliding filenames.

An empty Boolean result is valid for `inspect` and `check`, with zero volume and
null bounds. `build` reports an explicit export error because this STEP writer
requires a solid; no previous artifact generation is replaced.

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
structure and enforce **every** global face assertion and face-derived datum;
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
                  │
                  ▼
 cad-kernel: lines + planes, oriented B-rep, coordinate-cell operations
                  │
          ┌───────┴────────┐
          ▼                ▼
 cad-export             cad-preview
 STEP / STL             SVG / interactive HTML
```

A separate `cad-assembly` crate validates shared part definitions and rigid
occurrences. It preserves logical identity, local geometry reuse, and placement
metadata independently of the feature DAG.

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
paging, geometric LOD and occlusion culling remain future work. The streaming viewer needs HTTP(S) and Web Crypto; localhost works.

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
  through-slots, and separated multiple solids in the orthogonal domain only.
- General planar validation supports one strictly convex solid with supporting
  outward face planes, or one matched-cap right prism with a simple outer ring and convex holes.
  Multiple general solids are explicitly
  unsupported in one kernel snapshot; assemblies retain separate definitions.
- Closed orientable shells, including through-hole topology.
- Explicit rejection of enclosed cavities, nonmanifold contacts, unsupported
  geometry, and computations outside the numerical/resource envelope.

Round-hole cutting in arbitrary solids, general rotated nonconvex modeling, sketch constraints, general
polygon union/cut and nonconvex polygon Booleans, nonconvex hole rings, general trimmed curved surfaces,
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
rotated shaft. These are separate solid parts; curved Booleans are not yet supported.

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

### Revolved stepped and tapered parts

`revolve(id, profile)` turns a polygon's `[radius, height]` coordinates through
360 degrees about +Z. The closed profile must be counterclockwise and stay at nonnegative radii.
Horizontal edges create planar shoulders, vertical edges create cylinders, and
sloped edges create conical frusta. It may touch the axis
along one interval or remain entirely away from it for an annular part. Up to
256 vertices are supported. Sloped edges require positive endpoint radii and
radial/axial spans above tolerance. Separate profile holes, cone apex
singularities and partial turns are currently rejected.

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
chord error, with a 200,000-patch resource limit for revolved meshes.

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
