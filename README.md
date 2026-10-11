# Headless CAD

**Write a recipe. Build a part. Inspect it. Export it.**

Our goal is an **Apache-2.0-licensed, headless, AI-native CAD system with an
original Rust kernel**, targeting commercial engineering workflows served by
Autodesk-class platforms. This repository is the initial working foundation;
its current orthogonal geometry limits are not the product's target scope.

Read the [target architecture](docs/architecture.md) and
[milestone roadmap](docs/roadmap.md). First-party code is licensed under
[Apache-2.0](LICENSE); dependencies retain their own licenses and notices.
See the [dependency license inventory](docs/dependency-licenses.md).

A local, CLI-first CAD product built around our own Rust B-rep kernel. Recipes
are plain text suitable for Git, results are inspectable JSON, and exports are
STEP and STL. The current modeling domain is **axis-aligned parts made from
rectangular solids**: brackets, fixtures, open trays, and simple assemblies.

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
B-rep. It does not enable rotated-solid Boolean operations in the kernel.
Parts must reference solid features. Assemblies support both part occurrences
and nested subassembly occurrences; joints and named datums remain planned.

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
| `extrude(id, profile, height)` | Extrude a rectangle along positive Z |
| `translate(id, body, offset)` | Translate by `[x, y, z]`; named `x`, `y`, `z` also supported |
| `union(id, left, right)` | Combine material |
| `cut(id, left, right)` | Remove right-hand material from the left |
| `intersect(id, left, right)` | Keep common material |
| `param(name, default)` | Declare a numeric parameter the CLI may override |
| `output(name, feature)` | Declare a solid result to inspect or export |

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

This is the initial orthogonal face-reference contract. It does not yet provide
edge/vertex selectors, adjacency predicates, datums, assembly-instance selectors,
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

History currently contains generated face roles; Boolean outputs use a boundary
role. General persistent face selection and split/merge/deletion history remain
future work. Public snapshots must be revalidated after external editing or
deserialization. STEP/STL exporters validate inputs.

## Supported domain and current limits

- Axis-aligned rectangular profiles and planar rectangular B-rep faces.
- Translation, unions, cuts, intersections, open pockets, through-slots, and
  separated multiple solids.
- Closed orientable shells, including through-hole topology.
- Explicit rejection of enclosed cavities, nonmanifold contacts, unsupported
  geometry, and computations outside the numerical/resource envelope.

Round holes, circles/cylinders, rotated-solid modeling, sketch constraints, general
polygon profiles, curved surfaces, fillets, offsets, NURBS, STEP import, and GPU
modeling are **not implemented**. STEP uses AP214 `FACETED_BREP` with planar
boundaries and millimetre units, with occurrence relationships for assemblies; it does not preserve the executable recipe.
STL is unitless by specification, with coordinates written in millimetres.

The working orthogonal baseline is the first milestone toward the commercial
CAD goal. Nested assemblies, scoped face references and shared display assets are implemented.
Next come named datums, reference-driven modeling, streamed display assets,
general planar/analytic geometry, and trim/seam correctness.
The [roadmap](docs/roadmap.md) defines release evidence
for curved modeling, constraints, large assemblies, and manufacturing workflows.

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
Python environment to run it. OCP is never a runtime dependency. CI runs format,
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
```

Build the bracket first and run `cargo build -p cad-cli` before the hosted test.
The hosted test starts temporary server/watch processes and checks live recipe
edits, automatic refresh, failed-build preservation, recovery, and STEP download.
The refresh regression simulates interleaved generations during edit/revert so
a manifest and preview from different builds cannot be combined.
Set `CAD_BIN` to test a different built executable. Set `CAD_BROWSER` to an
existing Chromium executable instead of installing a Playwright browser if
preferred. The depth test checks rendered visibility and picking against
independent ray/rectangle intersections. Node is only a test dependency; exported
viewers have no external dependencies.
