# Headless CAD — Experimental

> **Experimental research and development software.** Geometry support is bounded,
> APIs and artifact formats may change, and the commercial release gates remain
> open. This project is not yet production-ready CAD. Validate engineering and
> manufacturing outputs independently before relying on them.

**Write a recipe. Build a part. Inspect it. Export it.**

Our goal is an **Apache-2.0-licensed, headless, AI-native CAD system with an
original Rust kernel**, targeting commercial engineering workflows served by
Autodesk-class platforms. This repository is the initial working foundation;
its current bounded geometry support is not the product's target scope.

The [competitive release contract](docs/architecture.md#first-competitive-release-contract)
covers reliable revisions, large assemblies, mechanical constraints,
manufacturing deliverables and reproducible human/AI workflows. The overall
product goal remains incomplete as these capabilities are implemented and validated.
The [commercial release readiness ledger](docs/architecture.md#commercial-release-readiness)
records the five open product gates and the work that still blocks release.

The [architecture decision](docs/architecture.md#architecture-decision-commercial-engineering-and-virtualized-inspection)
turns the commercial-CAD and virtualized-geometry briefs into concrete subsystem
contracts and acceptance obligations.

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
also accept contained through/blind drilling, counterbores and conical countersinks. Intersection supports bounded,
well-conditioned convex planar solids, including rotated inputs.
Finite external shaft keyseats and internal hub through-keyways retain exact
cylindrical surfaces. Their bounded coaxial clearance queries certify actual
surviving surface witnesses, with independently validated STEP exchange.

Native equal-distance `chamfer` also trims a selected straight edge of a strictly
convex planar solid, retaining original face lineage and a named generated face.

Separated orthogonal solids now compose directly when their complete bounds are
strictly apart and native supports are exact. This avoids constructing a global
coordinate grid for disconnected stock while preserving topology and face lineage.
`examples/billet_batch.star` builds 64 separated billets, then applies a real local
trim; its 1–12 row/column parameters reach 144 solids. Full native validation and
ambiguous-plane rejection still apply, and overlapping Boolean grids remain bounded.

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
archive for the current platform with `./scripts/package.sh`. The script stages
and checks the complete archive before replacing the previous distribution, so
readers retain the old archive if packaging fails.
For an unpacked archive, use its included `./cad` executable instead. Native
archives target their build platform and libc; source installation is the
portable route across operating systems.

## Local AI agent interface (experimental)

On Linux, expose a fixed workspace to an MCP client over stdio:

```sh
cad mcp --workspace /absolute/path/to/your/cad-project
```

The client launches `cad` with arguments `["mcp", "--workspace", "/absolute/path/to/your/cad-project"]`.
Use `cad_snapshot` to capture a relative recipe and its locked modules, `cad_revise`
to propose new root source, and `cad_evaluate` to compile, inspect, or build that
immutable candidate. Native failures remain structured tool results so the client
can repair a proposal and try again. `cad_snapshots`, `cad_read_source`, `cad_jobs`, `cad_job`,
`cad_read_artifact`, and `cad_verify` provide captured source and integrity evidence;
`cad_capabilities` describes native operations and session limits.

Each worker runs the existing CAD executable in a private directory. Candidate
jobs never modify your source or replace accepted outputs. By default, files and
jobs are session-local and disappear when the MCP process exits. To retain local
snapshots and results across restarts, opt into a separate state directory:

```sh
cad mcp --workspace /absolute/path/to/your/cad-project --state-dir /absolute/path/to/cad-state
```

Durable evaluations require a `request_id` (1–128 bytes, without control
characters). Retrying the same ID with the same captured source, mode, output
selection, parameters and executable returns the recorded job. Reusing it for a
changed request refuses; use a new ID for new work or a new executable. Discover
captured sources with `cad_snapshots` and jobs with `cad_jobs` after reconnecting.
Interrupted work is reported as interrupted and is never silently rerun. The
state directory is exclusively locked and must be separate from the workspace.
The same snapshot, job and byte limits persist across restarts; restarting does
not clear durable retention. Garbage collection remains separate work. The viewer
remains read-only.
This local adapter is Linux-only for confined source capture, accepts numeric
parameter overrides, and limits sessions to one active tool, 32 snapshots and 16
jobs. Before evaluating a recipe, Linux workers enforce a 2 GiB virtual-address limit,
45/46-second CPU soft/hard limits, a 128 MiB individual-file ceiling, 128 open
files, and no core dumps. Stricter inherited limits are preserved. The separate
wall-time and aggregate-disk guards remain polling checks. This is not a filesystem
sandbox or a cgroup RSS quota. Workers also register parent-death termination
before admission, so an exited agent process cannot leave a model worker running.
Hosted workers, module editing, external release delivery and automated retention
management remain open. See [the agent contract](docs/architecture.md#local-agent-candidate-jobs)
for the precise boundary.

From a source checkout, run the full rotating-drive client fixture after building
the release binary:

```sh
python3 scripts/agent_drive_demo.py --out-dir out/agent_drive_demo
```

It downloads verified before/after deliveries through MCP, preserves an invalid
proposal's native diagnostic, and replays captured source after the MCP process
exits. Open `http://localhost:8080/agent_drive_demo/index.html` with the viewer
server running. This scripted fixture tests the interface; it does not claim
autonomous AI reasoning. Choose a new delivery directory for each run.

The durable recovery walkthrough is available from a source checkout:

```sh
python scripts/agent_recovery_demo.py --cad target/release/cad \
  --workspace "$PWD" --state-dir /tmp/cad-drive-recovery-state \
  --out-dir out/agent_recovery_demo
```

Use a new empty state directory and delivery directory. The client builds two
source revisions, records a rejected engineering edit, kills the server,
reconnects, and recovers the same three outcomes before downloading verified
inspection artifacts. It demonstrates local protocol recovery, not autonomous
design or acceptance of a production release.

Durable sessions can also record an accepted local candidate. Read the current
head with `cad_release`, then call `cad_accept` with `request_id`, `job_id`,
`manifest_digest`, and an explicit `expected_base_revision` (null for the first
acceptance). Only a freshly verified successful build can be accepted. A stale
base or a changed payload under an existing request ID refuses; an identical
retry returns its original acceptance, including after restart. This records
local review state without modifying recipes or delivering a manufacturing release.
The store retains at most 64 acceptance records.

To exercise acceptance, stale-proposal rejection and recovery on the full drive:

```sh
python scripts/agent_recovery_demo.py --cad target/release/cad \
  --workspace "$PWD" --state-dir /tmp/cad-drive-acceptance-state \
  --out-dir out/agent_acceptance_demo --accept-revisions
```

Open `http://localhost:8080/agent_acceptance_demo/index.html` with the viewer
server running. Use fresh state and delivery directories for each run.

For larger durable deliveries, `cad_open_generation_stream` pins a verified job
and manifest. Read inventoried files through `cad_read_generation_stream`, then
require `cad_close_generation_stream` to return `sealed: true` before publishing
the delivery. Reads authenticate their bytes; the final seal detects unrelated
artifact changes since open. Only one stream is active per session, and tokens
expire on close or process exit. `cad_read_artifact` retains its full-job check
on each call. Add `--stream-artifacts` to the walkthrough command above to use
the streaming path. [The delivery benchmark](tools/bench-agent-stream.py) compares
both paths for the same retained job and checks identical SHA-256 file inventories.

Builds spool completed exports into a private stage and publish only after the
complete generation verifies. This reduces memory retained across many outputs;
individual exporters and exact-body residency still have separate limits.

Add `--replay-accepted` alongside `--accept-revisions` to rebuild accepted revision
B after the agent has stopped. The walkthrough uses a pinned executable, verifies
the accepted manifest, restores captured source or locked modules with the recorded
parameters and output selection, and compares native geometry and engineering
artifacts in a fresh directory. It also uses sealed streaming delivery. The
comparison excludes whole manifests and browser HTML; this is reproducibility
evidence for a local candidate, not manufacturing certification.

Rust applications reusing `BuildEngine` get a bounded persistent geometry cache
(1,024 entries / 128 MiB of serialized-result logical bytes by default).
`with_cache_limits` configures it and `cache_usage` reports retention, pins and
evictions. This controls cache reuse, not live build geometry or total process RAM.

## Review a complete source revision

The experimental bearing-drive walkthrough changes shaft diameter from 12 to
14 mm and mounting pitch from 60 to 72 mm. It builds both revisions, records a
source patch and semantic diff, publishes native engineering checks and section
sheets with STEP/BOM artifacts, then rejects excessive clearance and endplay requirements
without changing the last valid generation. Finally it rebuilds the revised
captured recipe without the working source or geometry cache.

```sh
cargo build --release --locked -p cad-cli
python3 scripts/drive_module_demo.py --recipe examples/drive_module_rotation.star --out-dir out/rotating_drive_demo
./target/release/cad serve out --port 8080
```

Open `http://localhost:8080/rotating_drive_demo/index.html`. Select a shaft or key occurrence
in either assembly preview to inspect that revision's actual Starlark source.
The phase slider moves nine rotor occurrences together without editing or
rebuilding the model. Eleven named clearance pairs have native full-path
certificates; the display is an inspection aid, not the proof.
The walkthrough is a scripted source edit, not an autonomous AI demo. It shows
the reproducible workflow an agent can use. Its no-replace publication currently
requires Linux; use a new output directory for every run.

The assembly includes a stepped keyed shaft, keyed disc, separate key, flanged
bushes and actual retaining rings, grooves and washers. Native dimensions enforce
separate shaft and hub endplay stacks. Ring installation, rated torque, load
ratings, threads and complete manufacturing specification remain design work. Native size and clearance checks cover their
named obligations, not whole-assembly certification. The five commercial release
gates remain open.

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

Selecting an occurrence in a new Starlark build opens its exact **Part declaration**,
including declarations in locked modules. **Body feature** jumps to the geometry
call; helper-call buttons show the corresponding invocation chain. The highlighted
range comes from evaluated Starlark AST spans, never a text search or a recipe
reconstructed from the mesh. The source-file selector still exposes every complete
captured file. `source-provenance.json` binds those locations to source digests and
the build context; the viewer verifies the captured bytes before enabling jumps.
Source navigation works in the portable, GPU and progressive inspectors and does
not run modeling calls. Older artifacts and JSON recipes retain whole-source
inspection. Comment-only edits update locations while preserving native geometry
and cache identities. [Retained checks](docs/evidence/source-provenance-local.json)
cover exact spans and replay; [browser checks](docs/evidence/source-provenance-browser-local.json)
cover all three inspectors and source verification races.

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
simultaneous mates, nonlinear constraint solving and collision checking remain
unsupported. Prescribed one-axis joints are available as described below.

### Solved planar mechanisms

`examples/solved_four_bar.star` defines a closed four-bar linkage through named
pins, rigid link lengths, grounded pins and an input angle. Pin coordinates are
initial guesses. The original numerical solver finds the remaining coordinates;
parts and rigid subassemblies take their placements from the solved link frames.

```python
layout = mechanism(id = "drive", pins = pins, links = links,
    drives = [{"id":"input", "link":"crank", "angle_radians":angle}],
    branches = [{"id":"upper", "pins":["b","d","c"], "side":"left"}])
mechanism_bind(assembly = linkage, instance = "coupler",
               mechanism = layout, link = "coupler")
```

Each pin has `id`, `initial: [x,y]`, and optional `ground: [x,y]`; each link has
`id`, `from`, `to`, and positive `length`. An angle drive measures from local +X
in radians in `(-pi, pi]`. A branch witness requires its third pin to remain on
the declared side of the directed first-to-second pin line. Guesses select a
local solution; a wrong or numerically unresolved requested branch fails.

A binding composes `mechanism frame * solved link frame * instance offset`.
The link's first pin is its origin and local +X points toward the second. The
mechanism frame accepts ordinary `offset` and quaternion `rotation`, so the
solved plane can be oriented in an assembly. Binding and datum attachment cannot
both own one occurrence. Other occurrences may attach to a bound component.

Every declared mechanism is a global build obligation, including selected-body
builds. Underconstraint, nonconvergence and rejected branches prevent publication;
failures retain solver evidence. Successful manifests include `mechanism_reports`,
`mechanism_bindings` and `mechanism_context_hash`. The hash binds source intent,
solver identity and results separately from native geometric assembly identity.
Motion edits reuse unchanged part geometry. STEP preserves resolved placements;
recipes and reports retain the mechanism intent.

Limits are 64 mechanisms and 4,096 bindings per recipe; each mechanism permits
30 pins, 64 links, 16 drives, 32 branch witnesses and at most 128 generated
constraints. Placement requires fully constrained local convergence and acceptable
conditioning. This is planar pin/bar closure, not general 3D mates, dynamics,
continuation through singularities, collision checking or proof of global uniqueness.

### Recorded motion studies

A `motion_study` follows a sequence of prescribed drive coordinates through an
existing planar mechanism. `examples/motion_four_bar.star` reuses the locked
four-bar source and records an outward and return stroke:

```python
motion_study(id = "cycle", mechanism = layout,
    targets = [{"input_angle":angle}, {"input_angle":1.3},
               {"input_angle":0.5}, {"input_angle":angle}],
    max_drive_step_radians = 0.08, max_pin_step_mm = 2)
```

Targets must provide every drive and the first must exactly match the source
mechanism's drive values. Each solve starts from the last accepted pin positions.
The sampler bisects intervals when a drive step, solved pin displacement or
failed solve requires it. Every accepted sample passes the same residual, rank,
conditioning and requested-branch checks as a static mechanism. Angles interpolate
numerically in the declared `(-pi, pi]` range; no automatic wrap or shortest-rotation
rule changes the requested path.

There are at most 128 requested targets, 512 attempted solves per study and 16
subdivision levels. A recipe permits 16 studies sharing 2,048 attempted solves;
failed trials also consume the budget. All studies are global build obligations,
including selected-body builds. Rejected studies retain an accepted prefix and
failure coordinates/diagnostics, and preserve the last published generation.

Builds publish `motion-<id>.json` and `motion-<id>.study.html`. The gallery links
the inspector, where you can scrub recorded pin/link configurations, inspect
residuals, select links and read captured source/modules. Playback uses stored
samples and makes no modeling calls. The JSON retains solved link frames for
headless consumers. Ordinary assembly/STEP artifacts keep the nominal recipe
pose; the study inspector is a planar mechanism layout, not animated solid CAD.

Motion intent and evidence enter `mechanism_context_hash` and publication/query
context. They leave unchanged part geometry and nominal assembly hashes intact.
Sampled continuation does not certify the intervals between samples, continuous
branch correctness, collision, clearance or dynamics.

### Prescribed revolute and prismatic joints

An attached part or rigid subassembly can prescribe one bounded coordinate:

```python
instance(id = "slider", part = carriage, attach_to = "rail",
         datum = "drive", target_datum = "drive",
         joint = prismatic(position = travel, min = 20, max = 100))
assembly_instance(id = "head", assembly = rotary_head,
                  attach_to = "slider", source_path = ["hub"],
                  datum = "axis", target_datum = "top",
                  joint = revolute(angle = angle, min = -3.14, max = 3.14))
```

Prismatic positions are millimetres and revolute angles are radians. Motion is
along/about local +Z after the existing attachment offset:
`target_world * offset * joint_motion * inverse(source_datum)`. The coordinate
and both inclusive limits must be finite. Joints require a complete attachment;
free joints and cyclic attachment graphs are rejected. At most 1,024 prescribed
joints may occur across all supplied reusable definitions. Structural validation
includes unused definitions and selected-output builds.

The strict JSON attachment field is `joint = {"kind":"revolute",
"position":0.6,"min":-3.14,"max":3.14}` (or `kind:"prismatic"`); omitting it
preserves fixed-attachment behavior. Reports retain limits, prescribed position,
local-definition frames and unit-labelled placement residuals. The nominal joint
has one intrinsic degree of freedom and zero remaining freedom after prescribing
its coordinate; this is forward kinematics, not a nonlinear constraint solver.
No collision, dynamics, loads or closed-loop consistency is certified. STEP
carries the resulting occurrence placements, not native joint intent.

`examples/jointed_linear_stage.star` combines a slider, a rotating rigid head,
semantic datums and a section anchored to the moving carriage. Parameters are
`travel` (20..100 mm), `angle` (-pi..pi radians), and `pose` (0, 1 or 2).
Motion edits reuse unchanged part solids while changing assembly identity and
resolved queries. The independent STEP/section checker is
`crates/cad-export/tools/check_jointed_stage_ocp.py`; its acceptance is tracked
separately from implementation and native tests.

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

Native assembly artifacts retain metadata. STEP preserves assembly geometry and
structure plus authored part numbers, descriptions and revisions. Material names
and density remain in native artifacts and the accompanying BOM. Materials do
not yet include certified property libraries, occurrence overrides or configurations.

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
Pure scalar `sqrt(value)` and `asin(value)` support authored formulas; inverse
sine returns radians. Inputs must be finite and within their mathematical domains.
These use the recorded executor's floating-point arithmetic, not symbolic algebra
or a claim of bit-identical transcendental results across arbitrary platforms.

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

### Inspect exchange fidelity

Full builds publish `<output>.exchange.json` beside each STEP/STL pair. The
report distinguishes supported, approximate, omitted and inapplicable semantics
for the current exporters. It binds the source output, captured recipe/context,
and the exact export bytes with BLAKE3 digests. It is included in the generation's
integrity inventory and the output's available files; display-only builds omit it.

Read this report before treating an export as an engineering handoff. STEP and
STL do not carry the same units, assembly structure or source intent. A native
attachment datum is not manufacturing GD&T, and a modeling uncertainty value is
not a dimensional tolerance. Unknown source provenance is reported explicitly.
These are exporter declarations with source inspection, not an independent
round-trip certificate. Source requirements can gate publication as described below; manufacturing PMI
remains future work. `cad verify` checks bytes, not engineering completeness.

### STEP part identity and revisions

Assembly STEP exports now carry source part numbers in `PRODUCT.id`, descriptions
in `PRODUCT.description`, and revisions in `PRODUCT_DEFINITION_FORMATION.id`.
`PRODUCT.name` retains the internal part ID: distinct definitions with the same
engineering part number remain distinct, while native geometry stays reusable.
STEP emits separate topology per logical part definition: sharing a single
B-rep across different products lets OCCT/XCAF collapse their identities. Repeated
occurrences of the same definition still share its STEP representation.
Missing metadata retains the legacy identifiers/default formation value and is
reported as absent source metadata, never inferred as an authored revision.

[step_metadata.star](examples/step_metadata.star) exercises duplicate part
numbers, different revisions, Unicode and quoted text, shared geometry and nested
occurrences. The independent OCCT oracle checks decoded product fields as well
as names, hierarchy, placements and unchanged geometry after metadata edits:

```sh
python crates/cad-export/tools/check_step_metadata_ocp.py --cad target/release/cad
```

Use the optional OCP environment described below. Fidelity reports and source
requirements now recognize these three fields as supported for STEP. STL still
omits them. Attachment datums, manufacturing PMI and recipe history remain
outside this STEP mapping.

### STEP material handoff

Assembly STEP carries material names and explicit density units per logical part
definition. Density values retain native kg/m³ units through a conversion-based
kilogram and a derived mass/length³ unit. Repeated occurrences reuse the same
material-bearing definition; a part without material remains unassigned.

[step_materials.star](examples/step_materials.star) exercises Unicode names,
shared geometry with different materials, repeated nested occurrences and an
unassigned part. The independent oracle checks decoded STEP units and imported
OCCT/XCAF material assignments, then changes density and verifies unchanged
geometry/STL alongside the expected native mass change:

```sh
python crates/cad-export/tools/check_step_materials_ocp.py --cad target/release/cad
```

`material_names` and `material_density` can be required for STEP publication.
STL omits both. This mapping carries names and scalar density, not a complete
material specification, mechanical properties, or manufacturing certification.

### Require exchange semantics before publication

Declare the handoff contract in Starlark (or schema-2 JSON IR):

```python
require_exchange(id = "units", output = "plate", format = "step", aspect = "units")
require_exchange(id = "solid", output = "plate", format = "step", aspect = "geometry")
```

See [exchange_contract.star](examples/exchange_contract.star) and the fourteen STEP
requirements in [round_bearing_cartridge.star](examples/round_bearing_cartridge.star).
`cad capabilities --type exchange_requirement` lists the accepted formats and
aspects. A requirement passes only when the exporter declares the aspect
`supported` and its inspected source count is positive. Approximate, omitted,
absent and unknown values fail. For metadata requirements, a positive count means
at least one definition authors that field and all supplied values are carried;
it does not require every part to author that field. Requiring STL units or manufacturing PMI therefore
fails with the current exporters.

Full `cad build` evaluates every declaration before atomic publication, including
cache hits. Every required output must be selected. Display-only publication
cannot satisfy these obligations. A failure returns `exchange_requirement_failed`
with the requirement and observed status/count, preserving the previous complete
generation. Successful manifest records bind each requirement to its source
output and fidelity-report digest. The captured recipe and context retain policy
intent independently of geometry identity. `cad diff` reports added, changed and
removed requirements even when output geometry stays unchanged.

`check`, `inspect`, `properties`, `bom`, `select`, and native SDK builds leave
publication requirements pending; successful geometry validation does not establish a released
exchange. This policy enforces current exporter declarations, not downstream
importer behavior or manufacturing certification. Drawings, GD&T and independent
semantic round trips remain separate release obligations.

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

Recipes have no ambient filesystem, network or clock API. Starlark `load` resolves
only explicitly supplied, locked local module sources (see below).
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
IDs remain stable. This legacy streaming viewer loads all occurrence metadata
at startup; the separate paged viewer below removes that requirement. Cluster
streaming and occlusion culling remain future work. The streaming
viewer needs HTTP(S) and Web Crypto; localhost works.

Assembly publication also includes `*.metadata.json`, a small source-bound index
into SHA-256-addressed spatial metadata pages. Branch pages reference two children;
leaf pages retain at most 128 occurrence records with paths, part identity and
display references. Each encoded page is capped at 1 MiB and included in the
generation's integrity inventory. This is the producer contract for paged
inspection; the legacy viewers still use their complete occurrence arrays.
The separate `*.paged.html` inspector consumes those pages on demand, retaining
at most 2,048 occurrence records, 256 metadata pages and 8 MiB of encoded metadata
plus pending reservations. It renders unresolved branches as striped regions;
click a region or zoom in to load part identities and shared geometry. Selecting
a resolved surface exposes its occurrence, semantic face and captured recipe.
The mesh cache separately accounts for 64 MiB of encoded data plus geometry VBOs.
These limits do not measure total browser heap or headless build memory. Curved
parts bootstrap from coarse meshes and refine toward one physical framebuffer
pixel, including the shader/rebasing precision allowance. Each shared mesh meets
the most demanding visible occurrence. Verified surfaces remain visible while a
replacement loads; insufficient budget or finest available detail is reported.
Raster coverage and depth-buffer quantization are outside this error target.

`paged_workcell.star` is a compact hierarchical scale fixture. Its default has
400 occurrences; `--param stations=1000 --display-only` builds 100,000 occurrences
from one shared part definition. This tests assembly metadata scale, separately
from unique-part modeling complexity.

Reproduce the paged-client acceptance with the local server serving `out`:

```sh
cad build examples/paged_workcell.star --param stations=1000 --display-only --out-dir out/paged_scale
cad build examples/modular_linear_stage.star --out-dir out/paged_modules
node tools/preview-check/metadata-stream.cjs
CAD_BROWSER=/path/to/chromium node tools/preview-check/paged.cjs
cad build examples/spindle_line.star --param stations=1000 --param columns=40 --param variants=12 --display-only --out-dir out/paged_spindles
CAD_BROWSER=/path/to/chromium node tools/preview-check/paged-lod.cjs
```

The browser harness uses Playwright from `tools/preview-check` and defaults to
`http://localhost:8080`. It checks sparse overview, distant-region refinement,
return picks, source/module inspection, resource accounting, corrupt-page fallback
and mismatched source-generation rejection. The curved-part harness additionally
checks delayed fine-mesh fallback, stable face picks, and budget reduction/recovery.
SwiftShader results establish local
functional behavior, not a hardware frame-rate or total-memory release gate.

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
general rolling-ball fillets, offsets, NURBS, STEP import, and GPU
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
optional OCP environment for exchange acceptance. Independent arc variables, circle-to-circle tangency, branch-continuation policies and
nonlinear assembly joint solving remain future work.

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

### Constrained blind-hole patterns on named faces

[sketched_blind_housing.star](examples/sketched_blind_housing.star) places two
blind clamp holes in a rounded housing with a central through bore. The
`hole_pattern` operation solves an inline sketch on a named cap face, derives
radii from center/rim points, and cuts native circular walls and flat floors.
Changing pitch, radius or depth regenerates the exact geometry, named face checks,
section diameters, STEP and STL.

```sh
cargo run --release --locked -p cad-cli -- build examples/sketched_blind_housing.star \
  --out-dir out/sketched_blind_housing --param half_pitch=16 --param hole_depth=5
```

With the viewer server running, open
`http://localhost:8080/sketched_blind_housing/housing.preview.html`. The captured
Starlark remains the model source. `underside=1` drills the opposite cap;
`pose=1` or `pose=2` places the body before drilling and rotates the sketch frame
with it. Generated roles `hole_clamp_left_floor` and `hole_clamp_right_floor`
retain their identity after subsequent holes.

This experimental operation accepts 1–16 separated, contained circular blind
holes on one recognized prism cap, with a fully constrained sketch and positive
remaining floor thickness. The frame is fixed before the first cut. It rejects
unresolved constraints, contact with other bores, breakthrough and side drilling.
The independent [OCCT check](tools/check-sketched-blind-housing.py) compares actual
solids, volume and mass properties across cap, pose and parameter changes.
The retained [local oracle report](docs/evidence/sketched-blind-housing-local.json)
records eight top/bottom, identity/cyclic/oblique and parameter-change cases:
both directed STEP solid differences were 0 mm³ in every case, with volume,
centroid, inertia and captured-source replay checks passing. Seven invalid
constraint, tangency, floor, face and frame edits were refused while preserving
every previously published artifact byte. The focused
[viewer check](tools/preview-check/sketched-blind-housing.cjs) picked both actual
floors from each cap, displayed the captured recipe and two diameter labels,
and observed GET-only inspection without exact-geometry downloads. These are
nominal test results for the supported domain, not manufacturing certification.

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
Noncoaxial intersecting passages and general pockets remain future work.
Conical countersinks are covered by the axial machining extension below.

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


### Countersunk bearing covers

[countersink.star](examples/countersink.star) isolates a conical cut around one
through bore. [countersunk_bearing_cover.star](examples/countersunk_bearing_cover.star) extends
annular machining to native conical seats around six pilot holes per cover.
Two mirrored covers, twelve flush screw envelopes, two bearings, three locating
rods and a shaft form 20 occurrences sharing five definitions. The exposed
`flange_inspection` output shows the tapered seats and three closed socket floors.
Picking a part shows its captured Starlark recipe.

`bolt_pitch` (40..44 mm), `seat_depth` (2..4 mm), and `flange_height` (18..24 mm)
control the example. Edit the recipe and rebuild to revise the model. A changed
seat depth revises the native conical boundary and screw envelope; the viewer
remains an inspection client. The recipe carries native face/property assertions
and fourteen STEP handoff requirements.

The kernel composes coaxial cylindrical and conical sections with shared boundary
rings at continuous transitions. Full native moments are integrated from the
analytic geometry. Independent slice integrals, STEP import and closed-STL
checks cover the composition; `npm run test:countersink --prefix tools/preview-check`
checks cone picking, through passages and blind floors in all three viewers.
Noncoaxial intersections, boundary contact, enclosed cutters, conical apex cuts
and general pockets remain unsupported. These fasteners are geometric envelopes,
not certified thread, fit or preload models.

Query native planar sections without publishing a generation:

```sh
cad section examples/countersink.star --output countersunk --origin 0,0,10
cad section examples/countersink.star --output countersunk --origin 0,0,10 --format svg > section.svg
```

`--normal` defaults to `0,0,1` and `--x-direction` to `1,0,0`; both must be
unit vectors and perpendicular. JSON reports directed analytic line/arc loops,
face IDs, area in mm², centroid in plane coordinates, source and geometry hashes,
query frame, build identity, global assertion results, and pending exchange
requirements. SVG retains the source-bound report in its metadata. Assembly
queries retain each occurrence path and sum areas, including overlaps; they do
not compute a union. Sections use native boundaries, never a display mesh.
General planar sections and transverse cylinder/cone sections are supported;
planes at topology events and oblique curved intersections return an explicit
error. These inspection sections do not specify manufacturing tolerances or GD&T.

Declare a section in the recipe to regenerate it with every successful build:

```python
section_view(
    id = "seat", output = "countersunk", origin = [0, 0, 12-depth/2],
    min_area = seat_area-0.000001, max_area = seat_area+0.000001,
)
```

`normal` defaults to `[0,0,1]` and `x_direction` to `[1,0,0]`; supplied frames
must be finite, unit length and orthogonal. Optional `min_area` / `max_area` are
inclusive engineering assertions in mm². View IDs use ASCII letters, digits,
underscores or hyphens. A recipe can declare at most 128 views; capability
discovery reports the aggregate geometry-work limit.

For an assembly output, anchor the plane to a named part datum at an exact leaf
occurrence path:

```python
section_view(
    id = "mounted_seat", output = "assembly",
    instance_path = ["module", "cover"], datum = "seat",
    origin = [0, 0, -1], normal = [0, 0, 1], x_direction = [1, 0, 0],
)
```

Supply `instance_path` and `datum` together. Path components are literal,
case-sensitive instance IDs from the output assembly root to a leaf; prefixes,
wildcards and leaf-name searches are not accepted. The plane coordinates are
local to that datum, then transformed through the occurrence into the assembly
frame. The view still sections the whole output assembly, with overlapping
occurrence areas summed. Without an anchor, coordinates remain in the output
frame. JSON IR records the pair as `anchor: {"instance_path": [...], "datum": "seat"}`.

Published JSON, SVG metadata and the manifest retain an `anchor_witness` with
the exact path, part identity/content hash, datum and occurrence frames, composed
world frame and resolved plane. A datum derived from a semantic face selection
also retains `semantic_datum` and its selection/content witnesses. Anchor intent
and the resolved witness participate in query identity, so changing the named
datum remains reviewable even when its plane is geometrically identical.
Placement or datum edits re-evaluate the view while reusing unchanged native
geometry. Missing paths/datums and planes that hit unsupported topology events
fail globally, as do area bounds on the resulting complete section.

Add a projected dimension after declaring its assembly section:

```python
section_dimension(
    view = "mounted_seat", id = "station_pitch",
    from_instance_path = ["left", "cover"], from_datum = "seat",
    to_instance_path = ["right", "cover"], to_datum = "seat",
    axis = "x", offset = -30, min = 89.999999, max = 90.000001,
)
```

Dimensions project the two named datum origins onto the resolved view plane.
`axis` is `"x"` or `"y"`; the value is the absolute projected separation, not
3D distance. The signed separation and both projected points remain in the
report. `offset` is the absolute perpendicular coordinate of the dimension line
in view millimetres. Datum origins may lie outside the section plane.
`min` and `max` are optional inclusive design assertions, not manufacturing
fits or tolerance annotations. Zero-length projections at modeling tolerance,
missing or ambiguous references, unsupported numeric precision and failed bounds
reject publication globally. Each view permits 32 dimensions and a recipe 1024.

JSON IR stores declarations in the view's `dimensions` array, with `from` and
`to` anchor objects. JSON/SVG/manifest reports retain both resolved datum witnesses,
including semantic face provenance. SVG adds extension lines, arrows and a
measured label; the canvas includes their extents. Labels round to six decimal
places (scientific notation for tiny nonzero values); metadata retains the
computed value. The `datum_section_fixture`
example checks station pitch in a rotated, repeated assembly. These are
associative inspection dimensions; full drawing sheets, fits and GD&T remain open.

For a radius or diameter, select a generated face through feature lineage:

```python
section_circle_dimension(
    view = "seat", id = "seat_diameter",
    from_feature = sink_body, role = "conical_faces",
    kind = "diameter", label_offset = [0, 30],
    min = 9.999999, max = 10.000001,
)
```

This measures the native intersection of that face with the section plane. A
conical seat therefore reports its diameter at the declared section depth, not
its maximum diameter. `kind` accepts `"radius"` or `"diameter"` (default).
Body views omit `instance_path`; assembly views require an exact leaf path.
The semantic selector must resolve to one face, and its section must form one
complete circle. Empty, partial, mixed-curve or ambiguous results fail explicitly.
`label_offset` is a vector from the measured circle centre in drawing millimetres;
its endpoint must lie outside the circle. Bounds remain inclusive design checks.

Circle reports retain the face selector, lineage, content hashes, occurrence
frame where applicable, centre and measured radius/value. Their declarations
are stored in the view's `circle_dimensions` JSON array. Linear and circular
dimensions share IDs and the 32-per-view/1024-per-recipe limits. Both kinds are
global publication obligations and appear in SVG with their source-bound evidence.
See `countersink.star` for body dimensions and `datum_section_fixture.star` for
radius/diameter measurements of repeated, rotated housings. The bearing-cover
example selects `seat_cut_0/bore_wall` to dimension one patterned seat and
`stock/inner_wall` for the bearing bore; a shared cutter role alone can resolve
to several faces and must not silently select one.

`cad build` publishes `section-seat.json` and `section-seat.svg` in the same
managed generation as the captured recipe and model artifacts. Their metadata
binds source, effective parameters, target geometry, query and generation.
All declared views are global obligations, including selected-output builds,
cache hits and display-only builds. An invalid plane, unsupported intersection
or failed area bound preserves the previous complete generation. Removing a
declaration removes its artifacts on the next successful build. Recipe diff
reports view changes even when the geometry stays unchanged.

The countersink and bearing-cover examples declare parameter-driven body and
assembly views. Assembly sections retain occurrence identity and sum overlapping
material; these inspection artifacts are a foundation for future drawings.


### Source-owned drawing sheets

Place declared sections and their supported native dimensions on paper:

```python
drawing_sheet(
    id = "bearing_sections", title = "Bearing fixture sections", revision = "A",
    width = 420, height = 297,
    views = [
        {"view":"left_seat", "x":10, "y":10, "width":195, "height":240, "scale":0.8},
        {"view":"right_seat", "x":215, "y":10, "width":195, "height":240, "scale":0.8},
    ],
)
```

The referenced `section_view` declarations must already exist in Starlark.
Paper sizes and viewport coordinates are millimetres, measured from the top left
with X right and Y down. `scale` is explicit paper millimetres per model
millimetre: `0.8` prints a 10 mm model length as 8 mm. Viewports cannot overlap;
10 mm margins and an additional 25 mm bottom title block are reserved. Limits
are 16 sheets, 32 viewports per sheet and 256 total, with paper dimensions at
most 2000 mm. Each section appears at most once per sheet.

```sh
cad build examples/bearing_drawing.star --out-dir out/bearing_drawing
cad verify out/bearing_drawing
cad build out/bearing_drawing/recipe.bundle.json --out-dir out/bearing_drawing_replay
```

The example imports its fixture through a pinned local module. Publication adds
`drawing-bearing_sections.svg` and `.json`, binding the sheet revision, captured
source context, section snapshots and exact section/SVG byte digests. The
captured bundle supports offline replay without the original source files.

Build reports use `layout_validated`: references and paper layout passed, but
geometry and annotation fit at the requested scale remain unchecked. Publication
checks that fit and only then reports `published`. It does not silently shrink
or clip a view; failure preserves the previous generation, including on selected
builds. Changing only title or revision reuses native geometry.

These sheets compose native section curves and the existing datum and circle
dimensions. Optional dimensional tolerances are described below. Projected hidden-line views,
fit classes, GD&T and a complete approved manufacturing release remain open.

Sheets also accept `dimension_tables`, which print evaluated native
`datum_dimension` measurements. Each table declares `id`, `title`, `output`,
paper `x`, `y`, `width`, `height`, and `rows` containing a `dimension` ID and
display `label`. Values are measured from the referenced faces, not authored
table cells. Every row must belong to the table's assembly output.
There are at most eight tables per sheet, sixteen rows per table, and 128 rows
per recipe. Table bounds, text fit, references and overlap are validated;
invalid declarations preserve the previous published generation.

Table SVG values are rounded to three decimal places in millimetres and marked
as nominal geometry. Drawing JSON retains each complete native witness and the
digest of `datum-dimensions.json`, including source context, occurrence paths,
part identities and numerical bounds. Sheets with tables use drawing report
schema 2; sheets without tables retain schema 1.

The rotating drive's `end_cover_machining` A3 sheet combines three native
sections with fourteen measurements: four pilot depths, four remaining housing
thicknesses, two cover thicknesses, two flange thicknesses and two representative
screw-shank lengths. `pilot_depth` accepts 4–6 mm (default 5)
and `cover_thickness` accepts 1.5–3 mm (default 2). Screw placement and shank
length follow these parameters, leaving a nominal 0.5 mm gap at each pilot
floor while the heads remain seated on the covers. The assembly now has twenty-six
native datum dimensions, including its ten existing retention measurements.
The initial [independent STEP checks](docs/evidence/machining-tables-local.json) cover
five configurations, including both parameter extremes: all thirty exported
components match independently constructed solids, the ten table measurements
match exported face planes, and all four screw-tip gaps measure 0.5 mm. Ten
invalid edits preserve the preceding generation.
The [accepted revision replay](docs/evidence/machining-tables-replay-local.json)
matches 149 artifact files and 27 manifest fields after restart and an offline
rebuild.

A viewport can opt into paper-sized annotations with
`paper_annotation_font_mm` (2.5–4 mm), an optional `paper_caption`, and
`annotation_labels = [{"dimension":"native_dimension_id", "tag":"P1"}]`.
Every native section annotation requires exactly one short tag in a styled
view. Tags replace only the visible name; measured values and native IDs remain
bound to the section report. Text size is independent of the model's drawing
scale. Captions replace the small coordinate metadata rows on styled sheets;
full provenance remains in JSON. Labels that overlap or exceed their allocated
viewport reject publication. Standalone section exports keep their existing
presentation.

The [paper-annotation checks](docs/evidence/paper-annotations-local.json) verify
2.8 mm text at 0.5 and 0.8 model scales, native numeric values and short-tag
binding, and twelve publication-preserving refusals. Scale-only and tag-only
source revisions leave all 38 compared STEP/B-rep files unchanged. A fresh
[accepted drawing replay](docs/evidence/paper-annotations-replay-local.json)
matches 149 artifacts and 27 manifest fields.

Tables can opt into `show_size_limits = True`. The separate design-limit column
comes from each native datum dimension's authored `tolerance` report. These
limits retain their full decimal value; only the measured nominal column is
rounded. Rows without an authored band say `not specified`. A value that cannot
fit at the declared text size rejects publication instead of truncating limits.

The drive declares independent size bands of ±0.10 mm for pilot depth and screw
shank length, and ±0.05 mm for flange and cover thickness. Four native stacks
calculate `pilot depth + flange + cover − screw shank`, with a nominal 0.5 mm
allowance and outward bounds of approximately 0.20–0.80 mm. Each must stay
within 0.15–0.85 mm. This calculation assumes the modeled flange, cover and screw
head seating; it does not establish actual assembly contact, thread engagement,
preload or manufactured-part conformance.

The [axial size-band evidence](docs/evidence/axial-size-bands-local.json) covers
five parameter configurations with independent STEP flange/shank face
measurements and decimal interval calculations. Thirteen invalid edits preserve
the previous generation, including an enlarged shank band that loses the
required allowance. The [accepted axial replay](docs/evidence/axial-size-bands-replay-local.json)
matches 149 files and 27 manifest fields after restart and offline rebuild.


### Native obround mounting slots

`slot_pattern` cuts up to four named, full-height capsules in a convex polygon
or supported convex rounded prism. Each capsule retains two planar walls and
two semicylindrical walls; mass properties, sections, meshes and STEP derive
from the native solid. Centers and unit directions belong to an explicit
source frame on a stock cap plane. The default frame is model XYZ; transformed
stock needs a correspondingly transformed frame. Its origin need not lie inside
cap material.

See `examples/obround_slot_plate.star`. The drive base uses four radius-3 mm
slots with center spacing controlled by `base_slot_length` (2–8 mm, default 6).
The existing four corner fillets and their semantic radius dimensions survive.
Only the slot feature needs recomputing when slot length changes.

Capsules must be strictly separated from each other and circular holes, and
inside a conservative polygon inscribed in the outer boundary. Near rounded
corners this may refuse geometrically valid cuts. Existing slot-bearing stock,
blind slots, concave boundaries and overlapping cuts are outside this operation.
Subsequent unsupported machining and clearance queries explicitly refuse these
bodies. This adds a bounded native machining capability, not general Booleans.

The [slot oracle](docs/evidence/obround-slots-local.json) retains nine
geometry cases and nine publication-preserving refusals, with independent
capsule moments, oblique source frames and zero bidirectional STEP volume
difference. The [complete drive checks](docs/evidence/slotted-drive-local.json)
cover five configurations with slot center spacings from 2 to 8 mm and
22 rejected edits. Slot-length-only changes rebuild one solid feature.
The [accepted offline replay](docs/evidence/slotted-base-replay-local.json)
matches 150 artifacts and 28 manifest fields. These bounded checks leave
the five commercial release gates open.

### Native nominal seating

`nominal_seating_check` binds exact assembly occurrence paths and semantic face
datums. It checks opposed native planar supports and a protected disk lying
inside both projected face trims, with planes coincident within arithmetic
uncertainty. Gap bounds describe numerical uncertainty; resolvable
gaps, penetration, tilt, insufficient material and unsupported faces reject the
build. The bounded candidate search can refuse a valid contact.

See `examples/nominal_seating.star` for an isolated flange/plate example. The drive
module adds six obligations: two sleeve-flange/cover seats and four screw-head
seats. Inspect `nominal-seating-checks.html` and its source-bound JSON report.
These are nominal local face checks, separate from whole-body collision checks,
contact forces, threads, preload and manufactured-part conformance. The viewer
only inspects the evidence; the Starlark recipe owns each requirement.

The native seating increment retains [independent STEP evidence](docs/evidence/native-seating-local.json) across five configurations, six
seating pairs per configuration and eighteen publication-preserving refusals.
The [accepted offline replay](docs/evidence/native-seating-replay-local.json)
matches 150 artifacts and 28 manifest fields after restart. These receipts cover
the bounded nominal planar seating domain; all five commercial gates remain open.

### Authored dimensional tolerances

Both `section_dimension` and `section_circle_dimension` accept an optional
`tolerance = {"nominal":6, "lower":0, "upper":0.02}`. Values are millimetres;
`lower` and `upper` are signed deviations from nominal. This example declares
6.00–6.02 mm manufacturing size limits. Symmetric, unilateral and same-sign
intervals are supported when their finite positive limits are numerically resolved.

The native design measurement must match `nominal` within the model's linear
tolerance. A broad manufacturing interval cannot authorize a different nominal.
Existing `min`/`max` remain independent design assertions. The generated report
retains the measured value, nominal, deviations and absolute limits; SVG displays
nominal followed by signed upper/lower deviations. Changing deviations reuses
geometry while updating source-bound section and drawing artifacts.

```sh
cad build examples/bearing_limits.star --out-dir out/bearing_limits
```

This fixture dimensions a 6 mm bore, 5.6 mm pin and configurable station pitch
through nested rotated occurrences. Its A4 sheet carries the three authored size
requirements. These are manufacturing instructions, not measurements of produced
parts, named fit classes, tolerance-stack analysis or GD&T. STEP currently carries
geometry; these annotations remain in the revision-bound JSON/SVG deliverables.

### Worst-case dimensional stacks

Check a source-declared linear relationship between toleranced native dimensions:

```python
tolerance_stack(id = "diametral_clearance", terms = [
    {"view":"pilot_limits", "dimension":"bore_size", "coefficient":1},
    {"view":"pilot_limits", "dimension":"pin_size", "coefficient":-1},
], min = 0.39, max = 0.45)
```

`bearing_clearance.star` extends the bore/pin drawing with this requirement.
Each term references a dimension with an authored tolerance. Signed coefficients
combine independent intervals; bounds may be negative for interference or other
signed size relationships. At least one of `min` and `max` is required. Limits
are 64 stacks, 32 terms per stack and 1024 terms total; duplicate references are
rejected and coefficient magnitude cannot exceed one million.

Every arithmetic step rounds outward over the represented binary64 source
values. The complete resulting interval must satisfy the requirement, without
kernel-tolerance slack. A boundary that cannot be established conservatively
fails. Nominal is an approximate descriptive sum; acceptance uses the outward
interval. The default example produces approximately 0.40–0.44 mm diametral
size clearance. Increasing allowances can fail even though the nominal model
and its rendered geometry are unchanged.

```sh
cad build examples/bearing_clearance.star --out-dir out/bearing_clearance
```

Open `tolerance-stacks.html` for a read-only report, also linked from the gallery.
`tolerance-stacks.json` binds stack results to captured source and section-report
digests. These checks are global during selected builds and cache hits; failures
preserve the previous published generation. Captured bundles replay the check.
The recipe owns the dimensional relationship: this does not establish alignment,
collision clearance, statistical/RSS behavior, named fits or GD&T.

### Native geometric clearance

Check the minimum geometric separation of two exact leaf occurrences:

```python
clearance_check(id = "jaw_separation", output = "clearance_station",
    left_instance_path = ["cell", "fixed"],
    right_instance_path = ["cell", "moving"], min_clearance = 2)
```

`clearance_station.star` uses two shared pentagonal jaws with independent local
rotation and a rotated parent assembly. Native vertex/face and edge/edge candidates
produce feasible witnesses. A separate all-vertex support calculation certifies
a lower distance bound; witness separation supplies the upper bound. Placement
arithmetic widens the interval. Acceptance uses the lower bound, not the visual
mesh, bounding-box distance or an unverified candidate estimate.

The supported domain is two nonempty, validated convex planar solids, recognized
finite solid cylinders, or supported convex full-turn linear-meridian revolutions,
including mixed pairs. Each operand is limited to 128 vertices, 256 edges and 128 single-loop
faces; recipes admit 64 checks and ten million path-resolution work units.
The convex path rejects other curved or nonconvex inputs (including annular tubes).
A separate coaxial material-region path is described below. Contact/intersection,
unresolved precision and resource excess fail explicitly. Failure does not classify collision or
penetration depth. Only declared pairs are checked; this is not whole-assembly
or continuous-motion certification and does not inflate manufactured tolerances.

```sh
cad build examples/clearance_station.star --out-dir out/clearance_station
```

The gallery links `clearance-checks.html`, a static inspector with distances,
occurrence paths and native witnesses. JSON records source context, part/assembly
hashes, frames and both distance bounds. Checks run globally during selected
builds and cache hits. Placement-only edits reuse part geometry; failed checks
preserve the previous generation and captured bundles replay the obligations.

`examples/keyed_clearance.star` adds bounded coaxial keyed-shaft and keyed-hub
pairs. The original round-stock separation supplies only a lower bound. The
`native_trim_witness` method finds points on material that survives both native
cuts and uses their actual distance for the upper bound. It accepts positive
radial, axial or combined gaps only when the padded interval meets the requested
accuracy. Different keyway orientations and opposed axes are supported;
noncoaxial keyed bodies, contact and uncertain witness membership are rejected.
The independent `tools/check-keyed-clearance.py` suite covers 15 configurations,
75 distance queries and six publication-preserving failures, comparing complete
STEP solids, boundary-face distances and actual native witness membership.

`examples/drive_module_keyed.star` incorporates a cut shaft, keyed disc and separate
key into the bearing-supported module while retaining its seven clearance checks,
material-volume/mass assertions, size dimensions and tolerance stacks. A shaft
diameter can span several descendant faces only when their section forms one
complete, unambiguous native circle; the report retains all face provenance.
Axial retention, standardized fits, rated loads and manufactured-part conformance
remain open.

`tools/check-keyed-drive-module.py` independently validates both the 12 mm/60 mm
and 14 mm/72 mm revisions, including all part volumes and mass, STEP hierarchy,
seven clearances, native dimensions and actual key contact/gaps.

```sh
python3 scripts/drive_module_demo.py --recipe examples/drive_module_keyed.star --out-dir out/keyed_drive_demo
```

`cylinder_clearance.star` extends the query to analytic roller pairs and a tilted
planar guard. Cylinder supports use recovered native axis, radius, height and
caps; neither seam vertices nor tessellation approximate the curved surface.
A bounded support search maintains convex-combination witnesses and checks an
independent analytic support-gap certificate. Failure to converge within 256
iterations is explicit. The existing native planar candidate path is retained.

```sh
cad build examples/cylinder_clearance.star --out-dir out/cylinder_clearance
```

The example varies relative roller tilt, axial/radial offsets and the whole
assembly pose. It also includes an intentionally unsupported noncoaxial `tube=1` case.
Clearance remains a nominal-geometry requirement, separate from dimensional
manufacturing envelopes and continuous-motion collision certification.

`revolved_clearance.star` adds tapered tooling: a frustum, a pointed cone, and a
convex crowned profile, paired with a cone and a tilted planar guard. Native
supports maximize over the complete revolved meridian, including circular extrema
and poles. Admission checks convexity before support queries; a hollow or waisted
profile cannot be substituted with its convex hull. Meridian corners must pass
strict convexity at modeling tolerance; redundant or unresolved collinear breaks
are rejected.

```sh
cad build examples/revolved_clearance.star --out-dir out/revolved_clearance
```

`shape=0`, `shape=1`, and `shape=2` select the supported profiles. `shape=3`
(waisted) and `shape=4` (hollow) deliberately exercise unsupported-input failures.
The same source-owned minimum requirement, placement precision limits and bounded
search apply. General curved/nonconvex clearance remains a separate capability.

`coaxial_clearance.star` checks a shaft inside an actual hollow bearing and the
axial gap to a spacer. It includes plain and stepped shafts, a tapered bore/shaft
pair, and a stepped internal bore. The native query compares filled radial/axial
material profiles, including concavity and holes in the resulting revolved solid.
It rejects intersecting boundaries and material containment before measuring the
remaining separation; a convex hull would incorrectly fill the bore.

```sh
cad build examples/coaxial_clearance.star --out-dir out/coaxial_clearance
```

The default stepped shaft has 1 mm nominal bore clearance and the spacer has a
6 mm axial gap. `shape=0..3`, `slide`, `opposed`, `pose`, and `axial_gap` exercise
profile and placement changes. `gap=0`, `oversize=1`, `eccentric=1`, and `tilt=1`
exercise contact, material containment and unsupported noncoaxial arrangements.
Small arithmetic alignment departures widen the coaxial interval. The coaxial
profile path rejects actual eccentricity or tilt rather than snapping axes. The
parallel circular-extrusion path below additionally supports lateral offsets.

The generic native API is `ModelSnapshot::clearance`; the original
`convex_clearance` API keeps its convex-only contract. Published clearance JSON
uses schema 3 and `kind: native_geometric_clearance`. Each query records
`method: convex_support | coaxial_meridian | parallel_annular`; its `axis` is a witness direction,
not a global separating plane for nested bodies. Coaxial admission allows at most
128 faces, 256 loops, two loops per face, and the existing vertex/edge/reference
budgets. The report displays the geometry method beside each checked pair.
General nonparallel or noncircular hollow/nonconvex queries and manufacturing
tolerance envelopes remain separate capabilities. Bounded straight-line motion
is checked by the declaration below.

`parallel_clearance.star` adds true lateral offsets between finite circular
extrusions, including a solid or hollow spindle and an offset annular sleeve.
The native query combines complete planar annular-material separation with the
finite axial-interval gap. When the radial footprints overlap but their heights
are separated, witnesses may lie at circle intersections away from the line
joining the centers; centerline sampling is insufficient.

```sh
cad build examples/parallel_clearance.star --out-dir out/parallel_clearance
```

The default spindle has 0.25 mm eccentricity and 0.75 mm bore clearance. The
sleeve has a 6 mm axial gap despite overlapping annular footprints. `eccentric`,
`lateral`, `shaft_radius`, `shaft_inner`, `slide`, and `spacer_x` exercise nesting,
external separation and axial/radial combinations. Opposed axes and common rigid
poses preserve the material sets. The report labels this path `Parallel annuli`;
coaxial and convex inputs retain their existing method labels. Contact/material
overlap and real axis tilt fail explicitly. This path does not extend offset
support to stepped or tapered meridians.

### Continuous straight-line clearance

```sh
cad build examples/translation_clearance.star --out-dir out/translation_clearance
```

Add `right_translation = [0, 0, 40]` to `clearance_check` to check the right
occurrence's entire travel in its **local body axes**, from time 0 to 1. The left
occurrence remains fixed. The example checks a stepped shaft through a bearing
and a carriage passing a guard. Its report shows `Continuous translation` and
bounds the minimum over the whole path; bounds need not be as tight as a static
query's modeling tolerance.

Native midpoint queries and conservative displacement bounds certify complete
interval coverage. Sampling alone never passes a path. `--param pass_y=0 --param
traverse=40` intentionally collides at quarter travel even though the endpoints
and midpoint are clear. Unsupported intermediate geometry, insufficient
clearance, numerical uncertainty and exhausted work budgets reject publication.
The previous valid generation remains available.

Vectors must be finite, nonzero and at most 1,000,000 mm long. Limits are 16 swept
checks per recipe, 1,025 native queries per check, and 4,096 queries across sweeps.
Reports retain interval certificates, timed witnesses and arithmetic allowances;
`query` still describes the initial pose. This supports one prescribed translating
body per pair, not rotation, general trajectories or manufactured tolerances.

### Continuous fixed-axis rotation clearance

```sh
cad build examples/rotation_clearance.star --out-dir out/rotation_clearance
```

Declare `right_rotation = {"pivot": [-20, 0, 0], "axis": [0, 0, 1],
"angle_radians": 6.283185307179586}` on a clearance check to validate a complete
turn. Pivot and unit axis are in the right occurrence's initial local frame;
the left body stays fixed. The signed angle declares the actual travel, including
a full turn, rather than the shortest rotation between endpoint orientations.
Rotation and translation declarations are mutually exclusive.

The example orbits a cylindrical tool past a radial guard and beneath an overhead
guard. Their default whole-path minima are 5 and 6 mm. `--param guard_radius=20`
introduces a quarter-turn collision despite clear endpoints and midpoint. The
native certificate bounds each interval using the body's conservative radius
from the pivot and angular travel, with explicit pose arithmetic allowances.

The report labels `Continuous rotation`, retains the initial query, and publishes
timed witnesses and coverage in `rotation_sweep`. Angles must be finite, nonzero
and at most one full turn in magnitude. Rotation shares the translation work
budgets. Unsupported intermediate geometry or a path that cannot be certified
within its budget fails without replacing the last valid generation. Combined
screw motion, coupled moving bodies and arbitrary trajectories remain unsupported.

### Prescribed rigid rotation groups

`examples/drive_module_rotation.star` extends the retained drive with one full
turn of its shaft, keyed disc, key, three washers and three open rings. The
source fixes the remaining twenty-one occurrences. `rigid_rotation_group` declares
exact leaf paths, a reference occurrence, and pivot/axis coordinates in that
reference body's frame; the builder resolves one shared world rotation.
A `clearance_check` can reference `rotation_group = "rotor"` instead of a
single-body translation or rotation.

The thirteen named positive-clearance pairs use 27 native distance queries.
Six pairs have both endpoints in the group, so common rigid motion preserves
their initial distance. Five pairs use a new rotational-symmetry certificate
against a stationary native circular extrusion or orthogonal full-turn
revolution. Complete boundary consistency, axis alignment and arithmetic
allowances certify the full path. Three actual native queries accompany each
symmetry proof; they are independent checks, not the continuous proof itself.
Stationary conical or generally trimmed solids retain generic subdivision.
Two additional cover-to-shaft pairs use native `bore_witness_orbit` proofs:
the actual polygonal covers retain their central through-bores and separate
screw-clearance holes. Their static distances are 2.5 mm on the left and 4.5 mm
on the right; source obligations require at least 2 mm and 4 mm respectively.

Clearance report schema 4 distinguishes `common_rigid_motion`, native rotational
symmetry and Lipschitz subdivision. A symmetry proof carries explicit `[0,1]`
coverage and no subdivision intervals; physical radius and speed remain nonzero.
The group report binds exact members, source/assembly identities, the shared path
and named check IDs. Unqueried members get bounded point-trajectory admission;
that does not certify their geometry contacts or clearance.

At most 16 groups, 64 members per group and 256 member paths per recipe are
admitted. Paths must resolve uniquely, group memberships cannot overlap on one
output, and each group requires a referencing check. Existing native query and
motion budgets remain enforced. Unsupported or uncertifiable paths fail globally,
including cache hits and selected outputs, without replacing valid artifacts.

Builds publish `rigid-rotation-groups.json`, its read-only report, and a
`rotation-<id>.html` inspector when a portable assembly preview is available.
The phase slider transforms only derived display points and normals. Captured
source, native dimensions and exported solids remain at the authored pose.
The inspector uses the captured generation instead of combining motion evidence
with live updates from another revision.

`tools/check-drive-rotation.py` independently checks signed full turns across
four parameter configurations, native proof identities, actual STEP sample
witnesses, thirteen distances at nine phases per case, all 435 occurrence pairs
at each phase, and failed-publication preservation. For the 54 new cover/screw
versus rotor pairs, separate conservative axial/radial bounds establish
full-turn separation with an explicit numerical allowance. These independent
bounds are distinct from the thirteen native named-pair certificates. Sampling
alone does not close collision or mechanical-release gates. Group membership
does not certify ring installation, strength, preload or unlisted contacts.

### Finite keyseats in round shafts

```sh
cad build examples/keyed_shaft.star --out-dir out/keyed_shaft
```

A bounded `cut` branch removes a rectangular keyseat from a solid circular
cylinder while retaining round journals at both ends. In the shaft's recovered
frame, the cutter spans `x = a..outside`, `y = -b..b`, and `z = start..end`.
The radial floor must satisfy `a > 0` and `a² + b² < radius²`; both axial ends
must stay strictly inside the shaft. The cylindrical exterior and pocket walls
remain native analytic geometry, with derived STEP/STL and preview artifacts.

The example exposes width, depth, axial position/length, shaft dimensions, scale
and output pose. Semantic assertions retain the pocket floor/end walls and both
journal caps through placement. This domain has flat pocket ends and sharp
corners; rounded milling ends, repeated seats and annular stock require
additional operations. Stepped solid stock has the separate bounded domain below. Matching internal hub keyways have the separate bounded
domain below. Native properties and transverse
sections are supported. Bounded coaxial keyed-body clearance uses certified surviving
native witnesses; general keyed-body distance queries remain unsupported.
A shaft keyseat alone does not establish a complete torque-transfer or retention design.

The independent `tools/check-keyseat.py` acceptance suite covers 12 scale/pose,
shallow-cut and authored-example cases, plus three failed-publication preservation
cases. It checks native and STEP volume, centroid and inertia, both directed solid
differences against an independently constructed cylinder-minus-box, and oriented
watertight STL within its declared chordal error bound. The browser regression
checks native floor-face picking and captured recipe source.

### Stepped shafts and nominal retention

```sh
cad build examples/retained_shaft.star --out-dir out/retained_shaft
```

A finite keyseat can now be cut into one constant-radius segment of a solid,
orthogonal revolved shaft while preserving its shoulders and retaining groove.
Seats crossing a radial transition, tapered or hollow stock, and repeated seats
are rejected. Native face lineage, full mass properties, sections, STEP geometry
and shared mesh boundaries follow the stepped shape.

The example assembles the shaft, two opposed flanged bushes, a washer and an
open C-ring. Native clearance checks use the actual stock meridians and surviving
boundary witnesses. Parameter revisions preserve the intended 0.20 mm nominal
axial endplay. Source-authored directed dimensions measure native semantic face planes, and a
worst-case dimensional stack enforces endplay globally, including cached and
selected-output builds.

`tools/check-retained-shaft.py` checks three installed configurations and six
isolated scale/pose cases against independent moments and imported STEP geometry,
including both directed solid differences, all installed part intersections,
thrust gaps, ring/groove contacts and oriented watertight STL. The viewer exposes
the captured recipe when any occurrence is selected.

This isolates nominal shaft retention assuming fixed bush seats. It does not
establish hub retention, housing anchorage, ring installation or elasticity,
standardized fits, or load capacity.

### Native axial dimensions and retained drive module

`datum_dimension` measures directed separation between parallel native planar
faces, selected through semantic datums and exact assembly occurrence paths.
Use `direction = 1` along the first face's outward normal or `-1` against it.
Reversed stops, tilted planes, plain coordinate datums and offset face datums
are rejected. Numerical bounds and authored size tolerances remain distinct.

```python
datum_dimension(
    id = "washer_width", output = "assembly",
    from_instance_path = ["washer"], from_datum = "start",
    to_instance_path = ["washer"], to_datum = "end", direction = -1,
    tolerance = {"nominal": 1.5, "lower": -0.01, "upper": 0.01},
)
```

A tolerance-stack term may reference `{"datum_dimension":"washer_width",
"coefficient":-1}`. Datum-only stacks enforce both the outward-rounded design
interval and the independently propagated native measurement interval.
Existing section-dimension stacks retain their design-interval semantics and
mark their measurement arithmetic as uncertified; mixing the two term families
in one stack is currently rejected. Builds publish `datum-dimensions.json` and
a read-only HTML report, with native face, placement and source witnesses.
Stack report schema 2 links the dimension evidence by digest.

`examples/drive_module_retained.star` composes the keyed drive with flanged bushes,
a stepped shaft, three actual retaining grooves, three open rings and three
washers. Its 24 occurrences use 11 part definitions. Ten native axial dimensions
feed separate shaft and hub endplay stacks: each has 0.20 mm nominal endplay,
with illustrative worst-case design intervals of 0.13–0.27 mm and 0.115–0.285 mm.
These are authored design limits, not supplier-certified fits or ring properties.

```sh
python3 scripts/drive_module_demo.py --recipe examples/drive_module_retained.star --out-dir out/retained_drive_demo
cad serve out --port 8080
```

Open `/retained_drive_demo/index.html` on the server. Review a source revision
from 12 to 14 mm shaft diameter and 60 to 72 mm mounting pitch, inspect the captured
recipe, and open axial-dimension and endplay reports. Both an excessive clearance
requirement and an excessive endplay requirement preserve the previous generation.
Captured source replays without the authoring file or geometry cache.

`tools/check-retained-drive-module.py` independently reconstructs three parameter
configurations, compares every exported STEP leaf in both directions, checks all
276 occurrence pairs for unintended solid overlap, and measures actual retention
contacts and gaps. Full assembly and material moments, BOM, native axial
dimensions and endplay intervals have separate expected results.

The installed geometry has explicit nominal shaft and hub retention. Ring spring
installation, rated axial/torque loads, thread engagement/preload, standardized
fits and manufacturing release remain open engineering work.

### Through-keyways in annular hubs

```sh
cad build examples/keyed_hub.star --out-dir out/keyed_hub
```

The matching hub operation uses `cut` with annular circular stock and a rectangular
cutter spanning beyond both end caps. The cutter opens the existing bore while
remaining strictly inside the outer wall. Its radial start lies within the bore,
its roof lies beyond the bore radius, and both roof corners retain resolved wall
thickness. Native cylindrical supports and line/arc trims define the resulting
keyed bore; the outer cylindrical wall remains intact.

The example exposes bore and outer radii, hub length, keyway width and roof,
cutter coverage, scale and common operand pose. Its default 4.04 mm width and
8.05 mm roof coordinate suit the planned 4 × 4 mm key seated at radius 4 mm in
a 6 mm shaft. These nominal dimensions do not establish load capacity or retention.
Blind keyways, repeated slots and rounded corners remain outside this domain.
Native clearance queries admit the separate bounded coaxial trim-witness domain.

`tools/check-hub-keyway.py` validates 14 scale/pose, shallow, thin-wall and
authored-example cases against independent closed-form moments and OCCT solids,
plus five rejected edits that preserve the prior publication. Exact STEP geometry
and oriented watertight STL have separate acceptance bounds.

`examples/keyed_coupling.star` combines a cut shaft, cut hub and separate key,
with a transverse engagement drawing. Its parameters revise shaft radius and
nominal bore, side and roof gaps. `tools/check-keyed-coupling.py` independently
checks actual imported face gaps, intentional floor contact, zero solid overlap
between parts and material accounting across four configurations. This is a geometric engagement fixture;
axial retention, toleranced fits and torque capacity still require engineering work.

### Native planar edge chamfers

```python
beveled = chamfer(
    id = "front_edge", body = stock,
    face_normals = [[0, -1, 0], [0, 0, 1]], distance = 1.5,
)
```

The two outward unit normals select exactly one planar face each in the input
model's coordinate frame. They must share one straight edge. `distance` is the
equal setback along each adjacent face, perpendicular to that edge; it is not
the cut plane's normal offset. Criteria are explicit directions, not face numbers
or nearest-face guesses. Rotate them with a rotated input body.

The kernel constructs the new bisector half-space directly and reconstructs
validated native plane/line topology. It supports one strictly convex planar
solid with at most 48 supporting planes including the new face. The cut must
remain local to the selected edge: every other original vertex stays strictly
inside, all original faces survive, and topology events reject. Curved or
nonconvex solids and unequal setbacks remain unsupported by chamfer. Longitudinal
fillets have a separate bounded operation described below. Numerical
conditioning and model tolerance can further reject otherwise nominal inputs.

Generated role `chamfer` identifies the new face; complete `body` lineage keeps
references to surviving faces. This supports chained chamfers, face assertions
and associative planar datums. Failed operations preserve the previous published
generation. [chamfered_guide_block.star](examples/chamfered_guide_block.star)
chains six chamfers, sections the result and mounts inclined wear pads through a
generated-face datum in a repeated assembly. Its 27-configuration corpus checks
volume, centroid and full inertia against independent domain integrals; separate
STEP checks verify all twelve support planes, section areas and pad placement.

Semantic chamfers can follow upstream edits and rotations through face lineage:

```python
bevel = chamfer_faces(id = "bevel", body = stock, faces = [
    {"from_feature": stock, "role": "wall_y_min"},
    {"from_feature": stock, "role": "end_cap"},
], distance = 1.5)
```

Both references must resolve to exactly one adjacent planar face on `body`.
Ambiguous, deleted or unrelated references fail before cached geometry can be
reused. The guide-block recipe uses these references for its two top bevels.

## Locked recipe modules

Shared Starlark functions live in ordinary Git-versioned `.star` files:

```python
load("recipe_modules/drive_parts.star", "rail_part", "carriage_part")
rail = rail_part("rail", length = 160, setback = 2)
```

The adjacent `modular_linear_stage.star.lock.json` pins each transitive module:

```json
{"schema_version":1,"modules":{"recipe_modules/drive_parts.star":{"blake3":"<64 lowercase hexadecimal characters>"}}}
```

Create or explicitly update the transitive pins after reviewing a source edit:

```sh
cad lock examples/modular_linear_stage.star
```

This command parses imports without evaluating modeling code, reports added,
changed and removed module pins, and atomically replaces a valid lockfile. Commit
the reviewed module edits and lockfile together in Git. Invalid imports, cycles
or unreadable sources leave the previous lock untouched; malformed or symlinked
lock destinations require correction before updating.

Module labels are relative to the root recipe directory, including loads inside
other modules. The lock command includes every transitive module. Builds verify
the pinned bytes before evaluation; they never fetch dependencies or update pins.
Cycles, unsafe paths, missing modules and digest mismatches fail without replacing
the previous generation. The Rust API accepts an explicit in-memory module map;
ordinary source-string evaluation cannot read files on its own.

Build `examples/modular_linear_stage.star` for two shared beveled rails and two
drilled carriages, with mass and section assertions. Its `recipe.bundle.json`
captures the root, lock and modules. Replay it without the original project:

```sh
cad build out/modular_linear_stage/recipe.bundle.json --out-dir replay
```

Use the same explicit `--param` overrides when reproducing a nondefault build.
The viewer's **Recipe → Source file** picker shows captured module text. This is
read-only source browsing; automatic feature-to-module line navigation remains
future work. Module and lock edits are watched alongside the root recipe.


### Native longitudinal fillets

`fillet` rounds one full-height convex corner of a recognized right prism. Two
semantic face references select its shared straight edge; the kernel produces an
exact tangent cylindrical wall, preserves existing face descendants, and records
the generated `fillet_wall` role. Original stock references can select subsequent
corners after earlier topology edits:

```python
rounded = fillet(
    id = "front_corner", body = stock,
    faces = [
        {"from_feature": stock, "role": "wall_x_min"},
        {"from_feature": stock, "role": "wall_y_min"},
    ],
    radius = 4,
)
```

The `rounded_bearing_cover.star` example rounds four corners, then cuts an actual
bearing bore and four mounting holes. Its source asserts volume, midsection area,
and survival of each fillet's semantic face. Parameters change corner radius,
bore radius and thickness; the viewer inspects the resulting geometry and recipe.

This operation supports convex line/arc right prisms with circular holes, selecting
a corner between two straight outer walls. It rejects cap edges, arc-adjacent
corners, reflex corners, polygon holes, and radius-driven topology changes.
It does not clamp the requested radius. General rolling-ball fillets remain open.
The independent `tools/check-longitudinal-fillet.py` acceptance uses OCCT's real
edge-filleting operation, closed-form box/quarter-disk moments, STEP material
differences, exact cylindrical radii, closed STL and captured-recipe replay.


### Measuring a fillet's native radius

`section_circle_dimension(..., kind = "radius", coverage = "single_arc")`
measures one partial native section arc selected through a semantic face role.
The rounded bearing cover uses its original `front_left` feature's `fillet_wall`
role after three further fillets and five drilled holes. Its radius bound follows
`corner_radius`; the annotation arrow touches the selected arc's midpoint.

The default `coverage = "full_circle"` keeps the existing complete-circle rule
for radius and diameter dimensions. Partial coverage rejects diameter requests,
multiple descendants or matching segments, lines, complete circles and unresolved
or numerically unsupported arcs. Reports retain the actual face, loop/segment,
angular span and arrow-point witness alongside source and occurrence identity.
These are nominal analytic support measurements, not manufactured tolerance or
physical uncertainty certificates. `tools/check-fillet-dimensions.py` independently
checks the imported STEP radius, native witness, SVG arrow and captured replay.


### Continuous clearance for rounded solids

The bounded native convex clearance solver now supports hole-free convex line/arc
right prisms. It evaluates trimmed arcs directly, using analytic support bounds
and native material witnesses. Admitted arcs are counterclockwise and no greater
than a semicircle; clockwise, concave, holed and unresolved supports reject.
This expands clearance queries without introducing a mesh-based distance proxy.

`rounded_rotor_fixture.star` places a 20×12×6 mm rounded rotor beside an R2
reference roller. A source-owned rotation group certifies its entire signed turn
for one named pair. The default R2 corners give an exact minimum gap of about
5.055728 mm against a 5 mm requirement. Editing the corners to R1 fails at an
interior orientation, despite 6 mm clearance at the start, midpoint and end.
The failed build preserves the prior accepted generation.

The inspector's phase slider moves display geometry; recipes and native checks
remain authoritative. `tools/check-rounded-rotor.py` compares every reported sample
with a closed-form distance and independent STEP geometry, checks witness
membership, and verifies the whole-path interval coverage. This fixture proves
clearance for the named pair; it does not model a contacting transmission.


The `drilled_rotor_fixture.star` extension cuts a real central bore and two
mounting holes into that rotor. Its continuous external-roller check uses the
explicit `convex_trim_witness` method. A raw outer envelope supplies a conservative
lower bound; a distance witness is accepted only when it survives every through
bore with the complete arithmetic and support-projection allowance. Geometry,
four corner radii and the central bore diameter remain source-bound.

This bounded path supports recognized curved line/arc prisms with circular
through holes and admitted convex partners. Overlapping outer envelopes,
witnesses inside or too near a hole, and unresolved numerical cases reject.
It does not establish clearance for a shaft nested inside the bore. Run
`tools/check-rounded-rotor.py --drilled` with the optional test oracle to compare
the actual drilled STEP solids, motion samples and native material witnesses.

The rotating drive (`examples/drive_module_rotation.star`) uses a rounded,
four-hole mounting plate. `base_corner_radius` (2–10 mm, default 6) controls
four native longitudinal fillets and their semantic radius dimensions on the
mounting drawing. Its material assertions include the removed corner volume;
the expanded 30-part assembly has thirteen named clearance checks and
nine-member rotation group. The viewer remains an inspector of the captured
recipe and evidence. These nominal dimensions do not certify manufactured fit.

The same drive now has two removable sleeve-retaining end covers and four
nominal screw envelopes. Each housing has its own outboard `hole_pattern`:
two constrained 3.5 mm blind pilots, 5 mm deep, aligned with the cover's
4.1 mm clearance holes. Opposite cap frames compensate for the housing's
asymmetric area centroid, so the screw axes stay at the authored positions.
The covers overlap the sleeve flanges by 1.5 mm radially while clearing the
rotating shaft hardware. Source checks include machined material volume,
pilot walls and floors, section diameters, and the unchanged shaft endplay.
The assembly uses 14 part definitions; left and right housings have distinct
geometry. Screw envelopes do not model threads, preload or rated retention.
The original eleven native continuous checks retain their named scope; two
additional native checks certify cover-to-shaft clearance for the full turn.
The exact covers are recognized as convex polygon prisms with circular through
bores. Blind bores, polygonal inner holes, nonconvex outer boundaries and
noncoaxial shafts remain outside this proof family.

The retained [independent geometry report](docs/evidence/drive-covers-local.json)
covers four configurations, including the 16 mm shaft and 90 mm mounting pitch.
All 30 STEP components match independent solids; the 54 added fixed/rotor pairs
have guarded full-turn lower bounds of at least 2.4999 mm. Nine sampled poses
per configuration show no positive-volume intersections among all 435 pairs.
The [accepted offline replay](docs/evidence/drive-covers-offline-replay-local.json)
matches 147 artifact files and 27 manifest fields after an agent restart.

The subsequent [native polygon-bore validation](docs/evidence/drive-polygon-bore-clearance-local.json)
checks all thirteen named drive proofs across four configurations. Separate
[plain and stepped shaft cases](docs/evidence/polygon-bore-clearance-local.json)
include a common oblique placement and seven publication-preserving refusals.
The [new accepted replay](docs/evidence/native-cover-clearance-replay-local.json)
also confirms that the added proofs leave all 51 native B-rep/STEP/STL assets
byte-identical to the prior drive, matched by output or part identity.

`examples/rounded_bearing_fit.star` pairs a rounded, through-bored housing with
a genuinely keyed shaft. Its static `bore_trim_witness` certificate checks the
nominal running gap using containing geometry for the lower bound and surviving
native surface witnesses for the upper bound. Two mounting holes remain real
voids. Captured recipes, STEP export and bore/journal dimensions support review;
this example does not establish load capacity, retention or a manufactured fit.
The optional OCCT test is `tools/check-rounded-bearing-fit.py`.

`examples/rounded_bearing_rotation.star` adds a full signed turn of that shaft.
The `bore_witness_orbit` certificate uses the complete bore/cap witness region
and a rotating untouched shaft ray to bound the gap over the whole arc. The
housing itself need not be rotationally symmetric. Three native queries retain
sample evidence; the analytic certificate supplies continuous coverage. Run
`tools/check-rounded-bearing-fit.py --rotation` for independent STEP checks of
both running and stepped-shoulder gaps, including reversed motion and poses.

### Inspect a mounting adjustment

`examples/drive_module_adjustment.star` adds a separate loosened mounting study
to the drive recipe: one fixed base and 29 upper occurrences moving together
through its slots. Four native bolt/slot checks cover the entire declared travel.
The smaller `examples/slotted_clamp_adjustment.star` isolates the same geometry.

```sh
cad build examples/drive_module_adjustment.star --out-dir out/drive_module_adjustment
cad serve out --port 8080
```

Open `drive_module_adjustment/translation-upper_slot_travel.html` in the local
viewer. The phase slider is inspection only; selecting a part exposes captured
Starlark. Model changes belong in the recipes. The study uses nominal loose
fastener envelopes and does not certify clamp preload or thread engagement.

The [accepted adjustment replay](docs/evidence/slot-adjustment-replay-local.json)
records full- and half-travel source revisions, a rejected overshoot that preserves
the accepted base, verified delivery streams and recovery after an agent restart.
Offline replay matches 87 native/engineering artifacts and 29 manifest fields.
Generate a fresh local comparison with:

```sh
python scripts/agent_adjustment_demo.py --cad target/release/cad \
  --state-dir /tmp/cad-adjustment-state --out-dir out/adjustment-demo
```

The client confines module loading to `examples/` and keeps its state directory
outside the source workspace.

The [independent slot-motion checks](docs/evidence/slot-adjustment-local.json)
cover five isolated cases, nine rejected revisions and two drive configurations.
They compare actual STEP boundaries and analytic full-path bounds, and sample
all 435 part pairs at nine phases per drive configuration without positive-volume
intersections. Continuous native proof remains scoped to the four named slot pairs.

### Inspect seated mounting endpoints

`examples/drive_module_tightened.star` places the same drive at either end of its
mounting slots. Eight native checks verify local seating of the four bolt heads
and nuts; four additional checks retain the shank-to-slot gaps. Select a part in
the preview to inspect its captured Starlark declaration.

```sh
cad build examples/drive_module_tightened.star --out-dir out/drive_module_tightened
cad build examples/drive_module_tightened.star --param mounting_endpoint=1 \
  --out-dir out/drive_module_tightened_right
```

These are nominal geometric seating witnesses. Threads, preload, friction and
rated mounting loads remain outside their scope. A positive `bolt_lift` or
`nut_drop` introduces a gap and fails the corresponding native seating check.
For a scripted accepted-revision demonstration with restart and offline replay:

```sh
python scripts/agent_tightened_demo.py --cad target/release/cad \
  --state-dir /tmp/cad-tightened-state --out-dir out/tightened-demo
```

The [accepted endpoint replay](docs/evidence/tightened-mounting-replay-local.json)
records a refused nut gap, two accepted endpoint revisions and recovery after
restart; offline replay matches 87 artifacts and 29 semantic manifest fields.
[Browser evidence](docs/evidence/tightened-mounting-browser-local.json) checks both
endpoint previews, native report links and exact captured source navigation.
The [independent STEP oracle](docs/evidence/tightened-mounting-local.json) checks
four endpoint configurations, eight seating disks and four slot gaps per
configuration, all 435 part pairs, and five publication-preserving refusals.

### Dimension the slotted base

`examples/drive_module_slot_drawing.star` imports the drive and adds a native
section drawing of its base. Each slot has measured width, overall length and
centerline length, with source-selected wall witnesses. Width and overall length
carry authored size limits; these do not certify manufactured parts.

```sh
cad build examples/drive_module_slot_drawing.star --out-dir out/drive_module_slot_drawing
cad build examples/drive_module_slot_drawing.star --param base_slot_length=8 \
  --out-dir out/drive_module_long_slots
```

Open `drawing-base_slot_machining.svg` for the paper sheet or
`section-base_slot_plan.json` for native measurements, centers, face witnesses and
arithmetic bounds. `slot_drawing_scale` controls the sheet scale; publication
refuses overlapping labels, slot lines crossing annotation labels, or geometry
that does not fit its allocated viewport.

The [independent slot-dimension checks](docs/evidence/slot-dimensions-local.json)
cover four geometry/paper configurations, actual STEP slot surfaces and SVG
annotation coordinates, exact replay, and seven publication-preserving refusals.
