# Target architecture: commercial-scale, AI-native headless CAD

Status: target design, with an implementation baseline and release gates below.
Updated: 2026-10-12. The project goal is active.

## Product goal

Build an **Apache-2.0-licensed, headless, AI-native CAD system with our own Rust
kernel**, targeting the engineering workflows served by commercial platforms
such as Autodesk Inventor and Fusion. The first market is mechanical parts,
assemblies, and manufacturing deliverables. Matching every Autodesk product is
not a single release criterion.

Engineering reliability, large-assembly performance, automation, reproducibility,
and manufacturing interoperability are the competitive measures. The current
orthogonal and planar solids, analytic extrusions, and full-turn piecewise-linear
revolutions are an executable starting point. Their limits do not define the
target product. Feature development must expand the example and
regression corpus toward increasingly demanding real engineering work.

The competitive target is Autodesk-class mechanical engineering, rather than
feature parity with FreeCAD. Success means a team can author, revise, validate,
inspect and release an engineering assembly from source. A fast viewer, a large
test count, or a recipe-to-STL demo alone does not satisfy that goal.

First-party kernel, recipe compiler, headless services, interchange, and viewer
are Apache-2.0. Third-party components retain their own licenses and notices;
release inventories must cover dependencies and redistributed assets. Essential
modeling and automation must work offline without proprietary kernel licenses,
paid AI services, or a vendor-hosted account.

Goal reaffirmed on 2026-10-12 from the commercial-CAD and virtualized-geometry
briefs: this is a product-level engineering ambition, covering the kernel,
regeneration, assemblies, manufacturing and team workflows. The project goal
remains active until a declared commercial workflow passes its release gates;
finishing the current prototype or display milestone does not complete it.

## Architectural commitments

### Product acceptance priorities

The commercial target is binding when choosing what to implement next. Evaluate
each increment against these five product outcomes:

1. **Trust a revision.** Change dimensions through a substantial feature history,
   preserve semantic intent, and identify failed obligations before publication.
2. **Work at assembly scale.** Inspect and revise a machine with bounded memory,
   selective exact-body evaluation, shared definitions and progressive display.
3. **Express mechanical intent.** Solve dimensions and mechanisms with explicit
   residuals, degrees of freedom and branch behavior instead of hand-positioning
   a visually plausible result.
4. **Release manufacturing evidence.** Regenerate associated drawings, dimensions,
   BOM and open exchange from the same validated source revision.
5. **Automate through source.** Let humans and agents propose reviewable recipe
   patches, consume structured failures and replay accepted builds offline.

The two supplied briefs inform these outcomes and the display contract below.
They do not make a particular cluster size, mesh-shader backend, silent healing
policy or proprietary solver a requirement. Progress must advance both the
drive-module engineering workflow and the separate assembly-scale workload.

### Decisions adopted from the feature brief

The product target is an engineering team releasing a revised mechanical design
from Git, with an inspectable chain from source intent to manufacturing evidence.
The following decisions govern implementation priorities, not just the viewer:

| Priority | Decision | Completion evidence |
| --- | --- | --- |
| Reliable engineering edits | Develop robust geometry, semantic references and incremental regeneration together | A changed bearing size survives the supported feature history or reports a precise unsupported/ambiguous obligation; no silently repaired dimensions |
| Large designs on bounded hardware | Independently schedule and budget exact bodies, assembly metadata, display assets and query results | Measure cold opening, one-part revision and release generation separately; count loaded bodies and peak memory, not only frames per second |
| Progressive GPU inspection | Build part LODs and paged metadata before a cluster-streaming backend; keep rendering replaceable | The staged M3 gates below pass with retained occurrence/source identity and explicit unresolved detail |
| Mechanical design intent | Solve sketches and rigid-frame mechanisms through headless services | Report residuals, degrees of freedom, conflicts and branch changes; geometry is requested separately for interference checks |
| Manufacturing and collaboration | Release drawings, BOM, validation and open exchange against one immutable revision | A revised release pack is reproducible offline and traceable to reviewed source and configuration |

Treat software architecture and hardware capacity as coupled constraints. Lazy
loading cannot eliminate the cost of building a new unique part, solving a hard
constraint system or exporting an entire detailed assembly. Profile those jobs
independently; faster display must not conceal an unbounded headless build.

### Invariants

1. **Code and recipes are authoritative.** Git stores source, parameters,
   dependency locks, design assertions, drawings' intent, and assembly intent.
   Geometry and display assets are rebuildable products.
2. **Our kernel owns geometry.** Supporting math, spatial, serialization, and
   graphics libraries are welcome. OCCT is currently an independent test oracle,
   not the implementation behind a Rust facade.
3. **The viewer is an inspection client.** Camera movement, picking, filtering,
   sections, source browsing, and display-asset fetching do not mutate designs.
   Modeling jobs originate from recipes through the headless CLI/API. Exact
   engineering results are produced by headless queries and may be displayed by
   the viewer; a visual approximation is never silently presented as exact.
4. **Separate design identity, geometric state, and display state.** A logical
   component or feature can keep its identity while its content changes.
   Array offsets, tessellation triangles, and cache keys are not design IDs.
5. **Load the representation a task needs.** Opening an assembly for inspection
   must not deserialize every exact solid. Repeated components share geometry
   and display assets.
6. **A failed operation does not publish invalid geometry.** Preserve the last
   complete valid generation, return structured diagnostics, and expose repair
   decisions. Never silently change dimensions to make a test pass.
7. **Tests are release evidence, not a proof over all geometry.** Combine precise
   contracts, robust algorithms, independent invariants, adversarial corpora,
   differential checks, and explicit unsupported/ambiguous outcomes.

### Competitive requirements and evidence

The supplied feature ideas become the following engineering requirements. Each
requires its own acceptance evidence; a successful viewer demo cannot close a
kernel, solver, or manufacturing milestone.

| Requirement | Architectural owner | Required evidence |
| --- | --- | --- |
| Large assemblies without loading all exact geometry | Assembly manifests, artifact service, residency manager | Overview loads zero exact bodies; benchmark repeated and unique parts separately under enforced memory budgets |
| Reliable edits through long feature histories | Semantic references, operation lineage, incremental builder | Topology-changing edit sequences preserve intent or report deletion/ambiguity; no silent retargeting |
| Robust modeling and controlled recovery | Original Rust kernel and explicit repair policy | Independent geometric invariants, adversarial cases, bounded repair deviations, retained last valid generation |
| Sketches and mechanisms with useful diagnostics | Separate sketch and rigid-frame constraint services | Residuals, degrees of freedom, conflicting constraints, singularities, and edit-continuation tests |
| Progressive inspection of detailed models | Meshing workers, display hierarchy, GPU viewer | Measured screen-space error, bounded residency, correct identity through refinement, usable coarse fallback during loading |
| Manufacturing and team workflows | Engineering queries, drawings/BOM, revision and exchange services | Associative deliverables, revision traceability, configuration-aware quantities, independently checked open exports |
| AI-native, source-driven automation | Typed recipe IR, capability protocol, isolated build jobs | Reviewable source patches, reproducible snapshots, structured failures, deterministic publication, no viewer mutation dependency |

These requirements govern the roadmap and example progression. Performance
targets below are proposed release gates, not results inferred from the notes.

### Reference engineering workflow

Use a configurable bearing-supported drive module as the first complete target
workflow: machined housing and cover, shaft, bearings, seals, repeated fasteners,
and mounting interfaces. Progress from today's shaft and cartridge examples to
bores, shoulders, fillets, fits, constrained motion, and manufacturing drawings.
The recipe declares design intent and engineering assertions; the viewer exposes
the resulting parts, references, and captured source for inspection.

A release candidate must survive a bearing-size change and a mounting-pattern
change through the full chain: affected features rebuild, references resolve or
fail explicitly, assembly placement and clearances are checked, and drawings,
BOM and STEP are regenerated against the same source revision. Retain expected
results for the original and revised configurations. A failed revision leaves
the last released generation inspectable and identifies the failed obligation.

This workflow exercises modeling depth. A separate machine/workcell assembly
exercises scale using repeated and unique definitions; cloning thousands of
fasteners cannot substitute for modeling or manufacturing acceptance. Record
both workflows in a capability evidence ledger with supported input domain,
known exclusions, test command, independent expected result, artifact revision,
and measured performance where applicable. Each entry is planned, experimental,
or validated for a stated release; passing one entry does not imply platform
parity or completion of the project goal.

The target evidence ledger is a versioned, machine-readable release input.
Each capability record must include its ID, status, supported domain, exclusions,
fixture, executable check, independent oracle, source/build revision and result
artifact digest. Performance records additionally pin hardware and workload.
CI must reject a transition to validated when required evidence is missing or
belongs to another revision. This ledger is planned; prose milestones and test
counts are not a substitute for implementing it.

## System decomposition

```mermaid
flowchart TD
    Source[Git: Starlark recipes, parameters, assertions, locks] --> Compiler[Recipe compiler and typed design IR]
    Rust[Rust SDK and agent protocol] --> Compiler
    Compiler --> Build[Incremental build scheduler]
    Build --> Kernel[Original Rust geometric kernel]
    Build --> Solver[Sketch and rigid-assembly solvers]
    Build --> Assembly[Assembly definitions and instance graph]
    Kernel --> Exact[Versioned exact geometry assets]
    Exact --> Query[Validated measurement and engineering queries]
    Exact --> Mesher[Bounded-error tessellation]
    Mesher --> Display[LOD clusters, edge overlays, spatial proxies]
    Assembly --> Manifest[Immutable assembly manifest]
    Exact --> Store[Content-addressed artifact store]
    Display --> Store
    Manifest --> Store
    Store --> Read[Read-only artifact and streaming service]
    Read --> Viewer[GPU inspection viewer and portable fallback]
    Exact --> Exchange[STEP and manufacturing exports]
    Assembly --> Exchange
    Query --> Deliverables[Drawings, PMI, BOM, validation reports]
    Assembly --> Deliverables
    Compiler --> Snapshot[Captured source and override snapshot]
    Snapshot --> Store
```

These are logical boundaries, not instructions to create empty crates or deploy
microservices immediately. Start as versioned Rust interfaces and local workers;
introduce process or network boundaries where isolation, parallel execution, or
artifact distribution justifies them. No kernel dependency on a browser, GPU,
window system, network, or language-model provider is permitted.

### Four distinct graphs

| Graph | Nodes and edges | Responsibility |
| --- | --- | --- |
| Feature/build DAG | Typed operations, inputs, source spans, assertions | Incremental deterministic evaluation and failure provenance |
| Topology and lineage | Vertices, oriented edge uses, loops, faces, shells, solids; generated/modified/split/merged/deleted relations | Solid correctness and persistent semantic references |
| Assembly instance graph | Part definitions, nested instances, rigid transforms, joints, configurations | Reuse, placement, BOM, motion, and selective loading |
| Derived display hierarchy | Shared part clusters, assembly proxies, refinement dependencies and page references | View-dependent coverage, streaming and bounded display residency; never authoritative geometry |

The geometric topology is not necessarily a DAG: boundary loops and incidence
are inherently cyclic. Constraints also form graphs with loops. An assembly is
not a Boolean union of its parts. Fixtures and enclosures use explicit part
instances; the modular rack reuses nested rows and carriages. Older examples can
still use disjoint unions. Named part frames and deterministic part/rigid-module datum
attachments are implemented; joints and nonlinear assembly constraint solving remain
target work.

Graph boundaries also define invalidation. A camera move changes display demand
only. An occurrence placement edit updates assembly bounds and dependent queries,
while reusing unchanged part geometry and display pages. A source dimension edit
invalidates affected feature descendants, their topology witnesses, and derived
assets. A meshing-policy change rebuilds display assets without changing design
identity or native engineering results. Include these four cases in integration
acceptance; caching must preserve the distinction even when all graphs live in
one process.

## Authoring and AI-native execution

### Target request and representation boundaries

These contracts describe the target runtime; they do not claim that all services
or residency controls exist today.

| Request | Required representation | Execution boundary |
| --- | --- | --- |
| Open, orbit, hide or isolate an assembly | Manifest, occurrence bounds and streamed display pages | Read-only inspection client; no exact-body loads or recipe evaluation |
| Pick a part and inspect its recipe | Generation-pinned occurrence identity, definition identity and captured source mapping | Read-only artifact fetch; show the actual Starlark source and effective overrides, with module/span links when available |
| Refine a coarse face selection | Display hierarchy and semantic mapping | Resolve a candidate set before reporting a face; picking never starts a modeling operation |
| Change a dimension or introduce a feature | Source patch, locked dependencies and expected base revision | CLI/agent build job evaluates affected dependencies and validation obligations; publishes atomically |
| Measure clearance, classify interference or create a precise section | Conservative spatial candidates, then required authoritative bodies | Explicit headless query job with accuracy, revision and evidence in its result |
| Solve assembly motion | Rigid frames, joints and constraints | Constraint worker; geometry loads only for separately requested collision checks |
| Release drawings, BOM and exchange files | Pinned design/configuration, exact geometry and annotation intent | Headless release job with validation and an exchange-loss report |

The viewer may display query artifacts, but selecting a part is not an implicit
request to run an AI agent, evaluate its recipe or mutate the design. Source
inspection must stay usable while detailed display pages are unavailable. Every
artifact links back to its source generation; the viewer must label an older
generation instead of pairing it with current working-tree source.

Keep independent lifecycle state for exact bodies, display pages and query
results. Evicting a display page cannot invalidate a geometric result; changing
geometry invalidates dependent queries and display artifacts even if the logical
part ID is unchanged. Cancellation releases reservations and cannot publish a
partial generation. Large-assembly tests must observe these boundaries directly,
including counting exact-body loads, rather than infer them from frame rate.

### Recipe and job protocol

Keep **Starlark as the initial authoring language**, with a typed, versioned IR as
the stable boundary and a native Rust SDK for library consumers. JSON is the
resolved interchange/debug form. YAML need not become a second programming
language. Reconsider a custom language only when concrete domain ergonomics
justify its parser, tooling, migration, and compatibility costs.

Evolve today's recipe frontend toward modules resolved from a checked lockfile,
dimension-aware values, named coordinate systems, declarative sketches, reusable
part definitions, patterns, instances, configurations, materials, and assertions.
External assets use explicit content references; evaluation does not fetch
unlocked dependencies or read ambient time, environment, or arbitrary files.

An agent works through the same public operation schemas and build protocol as a
human-authored recipe. Provide machine-readable capabilities, input ranges,
source locations, dependency explanations, cancellation, progress events, and
structured failure witnesses. The agent's proposed source patch is reviewable;
once accepted, evaluation is independent of which model proposed it. Persist
source and overrides with every published generation, not a generated geometric
approximation of the original recipe.

The future job API has explicit snapshot/expected-version inputs and idempotent
request keys. A worker reads one immutable source snapshot, returns artifacts and
validation, and publishes only a complete generation. Untrusted hosted recipes
run in isolated processes with CPU, memory, filesystem, and network limits; the
current Starlark interpreter limits alone are not this service boundary.

### Implemented capability discovery

`cad capabilities` and `cad_build::capability_catalog()` expose a deterministic
schema-2 catalog of current JSON IR feature operations, the recipe envelope,
profile/sketch types, nested assemblies, attachments, materials, face references
and engineering assertions, supported geometry domains, exclusions and resource
limits. Map and nullable argument types make schema 2 incompatible with strict
schema-1 catalog readers; recipe schema versions remain independent. Field
annotations identify recipe-version restrictions, including fields whose mere
presence is forbidden in recipe schema 1.
Implementation identities pin the catalog, kernel, executor, assembly and sketch solver;
the catalog fingerprint also changes when its own contract source changes. The CLI
adds its version, authoring guidance and recipe-command publication semantics.
Discovery requires no recipe, geometry, network or cache. Numeric/resource values
use implementation constants where available; executable parser-alignment and
example-compilation tests guard the curated contract against drift.

This is capability discovery, not a complete JSON Schema, a solver for input
feasibility or the target release-evidence ledger. Locked module resolution,
source-span mappings and hosted job controls are not implied by the catalog. Every input still passes structural and geometric
validation. Starlark call syntax remains distinct from JSON IR argument names.

`cad capabilities --operation feature.sketch` and
`cad capabilities --type assembly_definition` provide bounded focused views.
They include the transitive closure of inline object types, including map values,
nullable values and discriminator variants; logical references remain IDs. The
CLI reports `scope`, retains global limits and identities, and returns an error
for unknown IDs. Full-recipe corpus validation, parser defaults/nullability and
version-presence checks, deterministic output and side-effect-free focused
discovery guard this boundary.

The CLI retains `kernel_operation_failed` and the failing feature for native
operation failures, and now adds diagnostic text and implementation identities. It does not yet
retain a typed kernel failure subtype in the executor's public error. A protocol
acceptance test follows discovery, successful compilation, rejected geometry,
an explicit source patch, successful checks and deterministic artifact replay
without a model service or viewer mutation. That is infrastructure evidence;
an actual agent proposing and repairing a design remains a separate acceptance
obligation in the commercial workflow.

### Engineering assertions as build obligations

Schema-2 `assert_property` records a named output, a typed property with explicit
units, and at least one finite inclusive bound. It currently covers native body
and assembly volume, volume centroid and central volume inertia, plus assembly
mass, centre of mass and central mass inertia. Off-diagonal tensor entries are
negative product moments. Frames are the body output frame or root assembly
frame. Assembly properties sum occurrences, including overlaps; they do not
imply collision freedom. Body outputs have no material assignment, and every
assembly occurrence needs density for mass-weighted metrics.

All declared property assertions are global validation obligations, even on a
selected-output, display-only or cached build. Structural errors fail before
geometry. Demand planning includes the asserted output's dependency closure;
assertion-only assemblies are computed without adding them to published outputs.
Each distinct target is measured once. Before queries run, the builder reserves
a bounded topology/occurrence workload across all targets (ten million units,
at most 1,000 assertions), failing explicitly if the budget is exceeded.

Compare against the source interval without adding an implicit tolerance or
rounding a failing value. `out_of_range` and `unavailable` are distinct failures;
missing density, undefined empty-body centroids and query failures are never
interpreted as zero. A successful report pins the property, bounds, measured
value and target content hash. Build manifests retain these reports alongside
source and build identity; failure preserves the preceding artifact generation.
Semantic diff includes changed assertion limits independently of geometry edits.

Canvas, embedded GPU and streamed GPU previews carry these captured reports in
their recipe context. The context identity includes all property reports and
kernel/executor identities, so an unchanged selected body cannot retain stale
assembly evidence after another target changes. The read-only checks panel
distinguishes the displayed output from other named build outputs, shows exact
serialized measured values and inclusive bounds, and exposes target hashes.
Malformed or mismatched displayed-output evidence cannot render as passed.
Canvas refresh replaces checks with the generation; GPU views require reload.
Inspection never evaluates an assertion or fetches exact bodies to measure it.

The mounted drive module now has fixed material-volume, mass-budget and balance
limits. Its tests use independent density-weighted formulas across docking edits
and reject overweight/unbalanced variants during body-only builds. A separate
mixed-density two-cube oracle checks every scalar property, including nonzero
off-diagonals. This is numerical release evidence for supported native geometry,
not a substitute for general geometry robustness, fit/clearance queries, stress
analysis or manufacturing certification.

## Original Rust kernel

### Geometry and numerical policy

Separate geometric primitives/evaluators from oriented topology and modeling
operations. Extend initial line/circle and plane/cylinder support to ellipses,
other analytic surfaces, B-splines/NURBS, general trimmed curves/surfaces, seams, singularities,
and per-edge parameter-space curves. A 3D edge shared by two faces can have a
different parameter-space curve on each face.

“Exact geometry” here means the authoritative analytic/trimmed representation,
with declared numerical error and tolerance. It does not mean arbitrary CAD
intersections have symbolic or infinitely precise coordinates. Meshes and voxel
fields are derived representations with separately recorded error bounds.

Use robust orientation/incidence predicates, scaled numerical conditioning,
interval or adaptive-precision fallbacks where justified, and explicit error
budgets for intersections, projections, and trimming. Tolerances distinguish
model units, geometric uncertainty, solver residuals, and display error. Imported
local tolerances need provenance and permitted ceilings; a single global epsilon
cannot replace that policy.

### Modeling operations and repair

The general Boolean pipeline needs broad-phase candidate filtering, geometric
intersection, parameter-space splitting, consistent coincidence classification,
region selection, sewing/reconstruction, history propagation, and validation.
The current coordinate arrangement remains useful for the supported orthogonal
subset and as a test oracle; it is not the general curved-solid algorithm.

The first general planar increment is implemented: a native +Z extrusion of a
strictly convex CCW XY polygon with 3–256 vertices. Its authoritative caps remain
single planar polygons, with one quadrilateral side per input edge. Repeated,
collinear, crossed, nonconvex, and tolerance-uncertain profiles are rejected.
`start_cap`, `end_cap`, and `walls` roles accompany `wall_i`, which identifies
input edge `i → (i + 1) % n`; source vertex ordering is part of that role intent.

The initial general planar validator accepts one strictly convex solid: closed
oriented topology, convex planar faces, positive volume, and strict supporting
face half-spaces must agree. Multiple general solids in one snapshot are
explicitly unsupported. The existing orthogonal validator still handles its
nonconvex and disconnected domain. This planar branch retains orthogonal-only union/cut; intersection
also supports conditioned convex planar inputs as described below. A separate
parallel-cylinder Boolean branch is described below.
Assembly placement can rotate and reuse polygon solids without baking those
rotations into Boolean operands. `examples/angled_fixture.star` exercises this
boundary with triangular ramps, hexagonal seats, and an orthogonally cut base.
Independent analytic and OCCT checks cover triangle/trapezoid/pentagon measures;
`npm run test:polygon` checks the ramp silhouette and picking in the browser.
This is an M2 increment, not completion of general planar or analytic modeling.

Native rigid transforms now rotate line/plane geometry and vertices, then apply
translation. They preserve topology and explicit input-face lineage. General
rotations are validated for one strictly convex solid or one supported simple-profile
right prism; exactly represented signed
axis permutations additionally support existing orthogonal nonconvex and
multiple-solid inputs. No near-axis snapping or implicit quaternion normalization
is performed. Arithmetic uncertainty, translation loss, edge displacement and
postconditions have explicit checks. This does not enable general rotated
union/cut or arbitrary nonconvex planar validation. `tilted_jig.star` supplies
chained noncommuting rotations, source assertions and independent expected part
coordinates across parameter edits.

The first general convex intersection increment uses bounded plane half-space
reconstruction. Plane-triplet intersections form candidate vertices; active
support planes determine polygon faces and shared oriented topology. This keeps
analytic planes and face ancestry from both operands. Input support is currently
two strict single-convex solids with at most 48 total input planes; the existing
orthogonal intersection retains its nonconvex/multiple-solid domain. Conditioning,
incidence uncertainty and result validity are checked before publication. Unresolved
near-coincidence and tangent/lower-dimensional contact are explicit errors, not
snapped or silently repaired results. This is not general nonconvex Boolean support.

`clipped_tools.star` demonstrates octagonal cropping and two oblique shoulders,
with independent cross-section volumes and parameter-edit checks. The export
oracle additionally asks OCCT to compute Common from the original operand STEP
files, comparing area, volume, bounds and both directed difference volumes with
the native result. Browser depth/picking checks use independent analytic faces.

The right-prism increment supports simple nonconvex outer profiles as well as
convex ones, with optional polygonal through-holes. A simple CCW outer ring
encloses disjoint simple CW holes, with at
most 256 total vertices and 32 holes. Multi-loop cap faces remain authoritative;
wall faces preserve source-ring roles. Validation recognizes a matched-cap,
straight-wall prism and rejects unsupported general nonconvex geometry.
Concave hole rings use adaptive segment intersection and bounded-distance tests
against every other boundary, followed by winding checks in both directions to
reject nesting. This admits disjoint holes with overlapping convex hulls; convex
pairs retain their established separating-axis validation and numerical domain.
Clockwise winding, simplicity, strict separation and resolved turns remain
mandatory; there is no repair, snapping or tolerance welding. The existing total
vertex and hole bounds also bound the pairwise validation work. Mixed circular
holes use winding and distance to every polygon segment, including reentrant
bays. Native prism recognition revalidates this contract after rigid placement.
Signed boundary integration subtracts holes from volume, centroid, inertia and
face-frame centroids. STEP retains inner face bounds; derived display/STL
triangles preserve the openings and semantic face identity. The pinned ISC
`earcutr` dependency supplies triangulation candidates, independently checked
before use; it is not the modeling kernel. Adaptive orientation predicates from
permissively licensed `robust` check triangle signs and overlap. Aligned-hole
bridge T-junctions are restored at native vertices before boundary-incidence and
coverage checks. Prism correspondence follows native longitudinal edges and
rejects deviations beyond its arithmetic envelope; hole separation includes
operand-based error bounds. `perforated_mount.star` combines two
openings and a centroid-associated locator with independent planar-moment,
STEP and browser through-hole picking evidence. Preview APIs now propagate the
recipe tolerance. Multi-loop scenes use an explicit embedded capability marker:
new clients can refresh between plain and holed scenes, while older Canvas
clients reject the unsupported replacement and require a reload. Streamed
compatibility is enforced by the versioned mesh envelope described below.

`fork_mount.star` extends the corpus to a U-shaped cap with two arm openings.
Simple-ring validation checks nonadjacent segment crossings and numerical
separation; hole containment checks full boundary edges, not only hole vertices.
Nonconvex cap faces are recognized only as part of a complete matched-cap right
prism, including under rigid transforms. General nonconvex polyhedra and Booleans
remain unsupported. Signed cap fans are valid integration domains even when
individual terms are negative or zero; display/export use checked triangulation.
Independent signed-box moments and rotated inertia checks cover parameter edits.

Stream assets requiring explicit triangles now use a versioned mesh envelope.
An old fan-only client rejects this payload even when triangle counts equal a
fan; byte-count checks alone cannot detect a concave single-loop face. Bare
convex payloads remain compatible. New renderers preserve cap identity across
triangulation and pick through the notch and holes.

The first analytic increment is now a true circular extrusion: tagged line/circle
curves and plane/cylinder surfaces, two circular boundary edges, and one periodic
seam used in both directions. The validator certifies this canonical solid-cylinder
and concentric annular families and rigid placements; it does not yet certify
arbitrary reordered curved B-reps, curved intersections or general trimming. Volume, centroid, inertia,
bounds and cap datum frames come from the analytic boundary. Wall datum frames and general curved Booleans remain unsupported; the bounded
parallel-cylinder branch below extends Boolean support.

`shaft_stack.star` attaches a shaft, flange and end cap through cap datums and
exports a separately rotated shaft. STEP uses analytic circles/cylinders and seam
pcurves; OCCT independently checks solid validity, topology, volume and assembly
placements. Derived STL/display patches have a declared radial chord-error policy
(default 0.01 mm, maximum 4,096 segments), including an arithmetic allowance.
Display envelope 3 preserves native face IDs across wall patches, rejects older
clients, and records the achieved bound. Native JSON uses tagged geometry in
kernel 0.3; previous generated snapshots must be rebuilt from source.

The annular increment adds `circle(..., inner_radius=...)` without changing disk
recipes. Tubes retain four faces, six edges and four seam vertices: two annular
caps, outer and inward cylindrical walls, each with its own periodic seam.
Validation checks the full oriented topology and concentric support frames;
volume uses `(R-r)(R+r)` to avoid subtracting nearly equal squared radii.
Derived cap patches preserve the bore, and all bore-wall patches map to one
native `inner_wall` face. STEP retains inner bounds and inward surface sense.
`bearing_cartridge.star` exercises repeated retainer definitions, cap attachments,
mixed-material mass/inertia and bore/length edits. This is analytic annular
extrusion, not yet a generalized curved cut operation.

Progress through general nonconvex solid operations, curved extrusion,
revolution, generalized Booleans, sweeps/lofts, chamfers/fillets, offsets/shelling,
and controlled import repair. Include open shells and cavity orientation in the
data model before promising shelling. Each operation declares supported input
classes, degeneracies, maximum work, postconditions, and meaningful failures.

Healing is an explicit policy-driven operation: record changed entities, maximum
deviation, tolerance changes, and removed or merged features. Preserve an
unmodified imported source. A near-coincident repair must not erase intentional
clearance without a diagnostic and a policy that authorizes the deviation.

Repair acceptance includes chained and repeated operations. Measure cumulative
deviation against the preserved unmodified source, not only the preceding step.
Reapplying the same policy must be idempotent within its declared numerical
bound or report further changes explicitly. Test thin walls, fits and clearances
against their authorized engineering limits after the complete chain; passing
each operation's local tolerance is insufficient. A repair report cannot turn
an unresolved deviation bound into a successful manufacturing release.

### Persistent topology references

Use three explicit identity types: snapshot-local entity handles, persistent
semantic entity references, and assembly instance paths. A downstream recipe
references a semantic role or selector plus feature lineage, not `Face14`. Track operation-generated roles and many-to-many history:
unchanged, generated, split, merged, deleted, and unresolved. Cache reuse must
preserve instance-local provenance even when geometry is deduplicated.

Selectors combine semantic roles, named datums, ownership, adjacency, geometric
predicates, and expected cardinality. On regeneration they resolve to exactly the
required entities or return an ambiguity/deletion diagnostic with candidates.
Do not claim that a UUID alone solves topology changes or silently choose the
nearest face after a split. Store selector intent with source, and persist the
resolved witness in build reports for review and regression tests.

The current `cad select` query implements the first scoped contract: anchor a
generated face role to a logical feature, follow explicit orthogonal Boolean, translation and
rigid-transform lineage to a target feature, and require one, N, or all nonempty
descendants. Reports distinguish resolved, deleted, ambiguous, and unsupported,
with snapshot fingerprints and face/operation witnesses. This is recomputed from
source intent on each build; it is not persistent numbering. General geometric
predicates, edge/vertex selectors, instance-path selectors, and consumption by
downstream solid-modeling operations remain work to implement.

Starlark `assert_faces` persists this selector intent in schema-2 recipes. The
build resolves each assertion before artifact publication, including on cached
geometry; manifests retain successful witnesses and semantic diffs expose
assertion changes. This is the first build-time consumer of semantic references.
It does not yet attach a sketch or drive a geometric operation from a reference.

Mixed profile prisms now combine supported polygon boundaries with circular
through-holes. Validation certifies the underlying planar prism and each inward
cylindrical wall, including cap correspondence, containment and separation.
Native volume and inertia subtract analytic bore contributions; STEP retains
circle edges and cylindrical surfaces. Display/STL reuse sampled circle points
between caps and walls and independently certify cap triangulation. The combined
hole limit is 32; the derived cap limit is 2,048 boundary samples. This is a
bounded extrusion family, not general curved intersection or arbitrary trimmed
surface validation. `drilled_bearing_mount.star` composes these plates with
annular parts, associative cap placement and reused nested fasteners.

The revolution increment turns a simple piecewise-linear CCW meridian through
360 degrees about +Z. It creates a connected planar/cylindrical/conical B-rep with shared
circle edges and periodic seams, including stepped shafts, annular shoulders,
counterbores and blind axial bores. Radius is nonnegative; at most one continuous
axis-contact interval is supported. Source segment roles preserve feature intent.
Canonical recognition reconstructs the meridian from geometric supports and
compares topology and bounded geometry; it does not trust operation provenance.
Positive annular axial slices provide compensated volume/centroid/inertia
integration, with independent composite-cylinder formulas and OCCT STEP checks.
Planar datums use the axis-centered material centroid. Derived meshes share ring
samples across shoulder and wall faces. Sloped profiles support regular conical
frusta and conical ends terminating at the axis.
Positive polynomial coefficients integrate annular-frustum moments without
subtracting nearly equal fourth powers. Partial turns, arcs in revolved meridians, general trim
topology and general curved Booleans remain future work.
`stepped_spindle.star` exercises connected solids, parameter edits and analytic
STEP alongside an exploded assembly inspection view.

`Cone` stores a reference-plane origin/radius, an orthonormal frame, signed
radial slope and inward orientation. STEP uses a positive semi-angle and reverses
the support axis and seam parameter direction for narrowing cones; independent
imports check both taper directions, inward walls and oblique placement. The
native 3D seam and STEP pcurve agree under the axial-distance parameterization.
See the [STEP conical-surface definition](https://steptools.com/stds/smrl/data/resource_docs/geometric_and_topological_representation/sys/4_schema.htm).
Derived conical patches share exact sampled rings with their neighboring faces,
and exported facet normals match the chordal trapezoids or pole triangles. `tapered_nozzle.star`
extends the corpus with chamfers, countersinks and a parameterized long taper.

Conical poles use real vertices and a three-use boundary loop: one circular
edge and the periodic seam traversed in opposite directions. There is no
zero-radius circle, degenerate model edge or synthetic cap. The conical support
may have reference radius zero at its lower axial endpoint; canonical recognition
recovers that endpoint from the seam, then compares the complete model against
the reconstructed meridian. A single contiguous axis interval admits solid
cones, two-tip solids and pointed blind bores. Isolated axis contacts and pinched
shells remain rejected. Circle samples are shared with adjacent faces and pole
fans contain nondegenerate triangles. Independent cone formulas, topology
mutations, cyclic profile starts, scale changes, oblique transforms and external
STEP readback cover this bounded singularity contract.

`centering_fixture.star` integrates the capability into a between-centers layout:
two instances of a shared steel pin seat in opposite conical blind bores, with
two shared drilled holders, an aluminum workpiece and a base. Datum attachments
set the frames; these are prescribed placements, not contact or collision
certification. Parameter tests cover workpiece length, radial bore clearance and
tip depth. Changing only tip depth rebuilds two native definitions and conserves
total material volume while changing density-weighted mass. Recipe assertions
check native volume, mass budget and longitudinal balance. The viewer continues
to inspect captured source and validated evidence without running modeling tools.

`serpentine_manifold.star` advances profile extrusion with a nonconvex inner
channel boundary beside circular fastener bores. Its covered assembly has nine
occurrences and five part definitions; an `open_channel` configuration exposes
the passage for inspection. Width and depth edits each change two definitions.
Independent rectangle/cylinder subtraction checks volume, centroid and the full
inertia tensor; kernel tests additionally cover holes with overlapping convex
hulls, forbidden nesting/contact, bay circles, cyclic source starts and oblique
transforms. Channel dimensions and datum seating remain prescribed design intent,
not pressure/sealing or collision certification. The operation remains a native
right prism with through-holes, not a general curved Boolean operation.

The line/arc prism increment supports simple CCW outer paths with circular
through-holes. `PlanarPath` stores an exact closure point and ordered `LineTo` /
`ArcTo` segments; the native `CircularArc` retains a circle support, distinct
endpoint vertices and a signed sweep. Outer arcs can turn either direction
and are at most a half-turn; clockwise recess walls are inward cylinders. Tangent joins remain explicit source vertices. Line-only
paths lower to existing polygon prisms with `segment_i` aliases. These are bounded
profile extrusions, not general curved Booleans or partial revolutions.

Canonical recognition reconstructs the profile and frame from topology and
supports, validates analytic trimmed-curve simplicity, CCW turning and strict
hole containment/separation, then
compares the complete reconstructed B-rep. Signed Green integrals use analytic
Fourier-polynomial integration for curved boundaries; circular-hole subtraction
and centered integration provide exact volume, centroid and full inertia without
mesh measurements. Planar cap datums use material centroids. Native bounds include
trimmed-arc extrema after rigid placement.

Paired lower/upper trims share angular samples; native endpoint/support roundoff
is reserved alongside chord sagitta in the display budget. Independent cap
triangulation certification remains mandatory. A coarse approximation that
compromises a valid native clearance fails closed, leaving the native geometry
unchanged. Work remains bounded by 256 path segments, 32 circular holes and 2,048
sampled cap points. Kernel acceptance includes unequal-radius/open/major-arc
rejection, topology corruption, independent capsule/semicircle moments, oblique
bounds/tensors, scale changes and endpoint-budget regressions; separate STEP/STL
oracles cover capsules, rounded rectangles and asymmetric profiles with holes.

`rounded_clevis.star` integrates rounded outlines into five occurrences sharing
four definitions. Its parameter corpus checks independent link moments and
assembly material properties, bounds and datum placement; length/radius edits
rebuild one definition each and gap edits rebuild two. Property gates apply to
selected-body builds too. Independent STEP checks verify all five placements,
definition sharing, volume and bounds. `test:arc` checks 9,583 analytic rays
across capsule/rounded profiles, native/oblique placements and all three display
clients, including curved walls and circular bores. Captured source matches the
built recipe and inspection loads no exact geometry. The datum-prescribed pose does not
claim joint solving or mechanical load validation.

`split_bearing_block.star` extends this domain to an inward semicircular seat.
Two shared jaws, a stepped shaft and four reused sleeves form seven occurrences
from three definitions. Independent rectangle/semicircle/cylinder integrals cover
27 configurations and full native body/assembly inertia, including material mass
weighting. Gap-only edits reuse all geometry; seat-radius edits rebuild two
definitions and depth edits rebuild three. Recesses remain native analytic arcs
in STEP. Three independent STEP-import cases verify inward surface orientation,
closed STL and assembly placement; 12 browser views check 706 independent rays
and exact captured source without exact-geometry downloads. An invalid crossing
revision cannot replace the last valid publication.

Native simplicity uses finite trimmed line/line, line/circle and circle/circle
intersection and distance candidates. Adjacent segments may share their declared
endpoints; nonadjacent contact or unresolved separation is rejected. Analytic ray
winding determines circular-hole containment with explicit failure if ray
classification remains ambiguous. Clearance uncertainty includes support-circle
centers and radii, including nearly flat arcs with distant centers. Both convex
and concave outlines retain the
existing bounded segment/hole counts, support consistency and mandatory mesh
coverage checks. This broadens right-prism profiles, not general curved Boolean
operations, arc-shaped inner loops, fit verification or joint solving.

### Bounded parallel-cylinder Booleans

The native Boolean dispatcher now recognizes two solid circular cylinders with
parallel or opposite axes. It computes transverse circle intersections and
constructs trimmed line/arc-prism topology, without mesh classification. Union
requires matching axial intervals; intersection takes a resolved nonzero common
interval; difference requires a tool spanning the left operand. Strict
containment can produce an eccentric circular hole. Clearly disjoint intersection
is empty and difference is unchanged; disconnected union remains unsupported.
Annular inputs, arbitrary arc prisms, chained lens/crescent operands, partial-depth
cuts and skew axes are outside this branch. Tangency and uncertain separation
fail explicitly instead of generating nonmanifold contacts or snapping geometry.

Every source face has explicit surviving or deleted lineage. Exterior arcs
longer than a half-turn split into two native faces with the same source wall.
Cut walls inherit the tool wall with inward orientation; cut caps inherit the
left operand. Intersection caps inherit the source support defining that axial
limit; matching union/intersection caps can carry both contributors. Tests cover
partial overlaps, opposite-axis cap roles and common oblique transformations.

Support agreement uses a scale-dependent arithmetic envelope of 4,096 machine
epsilons, which must fit within one sixteenth of the linear modeling tolerance.
Axis agreement has a separate 128-epsilon gate. Axis-aligned cap coordinates
require literal equality; oblique reconstruction permits only arithmetic-envelope
agreement and certifies the resulting world supports against their contributors.
This is a floating-point consistency policy, not a proof of mathematical
coplanarity. It does not use the larger modeling tolerance to merge nearby caps.
Nonzero separations too small to resolve return a numerical/unsupported result.

`twin_lobe_fixture.star` composes union, two through-cuts and intersection into
a six-occurrence, five-definition tooling layout. Independent unequal-circle
segment integrals check 27 configurations, native volume/centroid/full inertia,
assembly material moments and selective invalidation. Publication tests preserve
the previous generation after tangent, partial-depth and unresolved-cap edits.
This advances the bearing-housing geometry track without closing general curved
intersection, contact, fits or manufacturing-release obligations.

### Sequential machining through-bores

A separate difference path cuts a solid aligned cylinder through a recognized
polygon or line/arc right prism. It preserves existing vertices, edges, walls
and face IDs, appends circular inner loops to the two caps, and adds one inward
cylindrical wall with a true periodic seam. All existing faces have identity
lineage; only the tool wall contributes to the new bore. Tool caps are deleted.
This lets later drill features retain earlier holes and the stock's seat/cap
references. Supported arc-prism results of cylinder Booleans can be drilled too;
this is useful composition without general Boolean closure.

Recognition and analytic profile separation certify that the added bore is
wholly in material and clear of outer boundaries and existing polygon/circular
holes. Placement arithmetic reserves a separate conservative clearance margin;
the actual radius and existing geometry are unchanged. The cylinder must cover
the full stock interval, with parallel or opposite axis, and resulting world
supports must meet the arithmetic policy before native validation. Reuse the
existing maximum of 32 combined holes. Outside cuts, holes entirely inside a
preexisting void, tangent/crossing/overlapping holes and skew cuts are explicit
unsupported outcomes. Blind cuts use the additional native family below. Neither tessellation nor tolerance snapping
decides native containment.

`machined_bearing_support.star` applies four cuts to a concave jaw before
assembling two shared jaws, a shaft and eight sleeves. Independent signed-area
integrals cover 27 configurations, full native/assembly moments and selective
invalidation. Publication acceptance checks global machining obligations even
when the requested output is unchanged stock. Extend toward interacting cuts,
general pockets and generalized face splitting to advance the manufacturing
workflow; the current assembly does not certify fits or contact loads.

### Flat-bottom blind cylindrical machining

A native bored-prism family extends machining to top and underside blind holes,
interleaved with through-bores. Each cut appends an inward cylindrical wall and,
for a blind cut, a planar floor; it adds an inner loop only to the opened cap.
All previous faces retain identity. The tool's wall and terminating cap map to
the new wall and floor; the opposite tool cap is deleted. Generated roles
`bore_wall` and `bore_floor` support downstream engineering obligations.

Recognition certifies the boundary itself. It peels canonical cylinder blocks,
checks topology/support/orientations and replays them against a recognized base
prism. No hidden construction metadata makes an otherwise invalid solid valid.
An internal full-height surrogate certifies projected profile clearance and
provides shared cap tessellation boundaries; it is never exported as the native
solid. Native mass properties subtract the actual removed cylinders from the
base, using full parallel-axis tensors. Display walls terminate at the true
floor, floor disks close the pocket, and unopened cap disks restore the original
outer face identity. Planar datum queries use actual cap areas and floor centers.

The cylindrical tool must open one or both stock caps with a certified parallel
axis. Blind depth and remaining material must exceed the tolerance plus reserved
arithmetic uncertainty. Full circular supports are checked against stock and
terminating tool planes; tolerance is not a snapping policy. Circular projections
must remain separated, including opposite-side pockets. This intentionally
excludes noncoaxial intersecting passages, enclosed cavities and arbitrary pocket
profiles. Coaxial counterbores use the separate boundary family below. The existing 32 combined profile-hole bound applies.

`blind_bore_pallet.star` exercises twelve consecutive machining features, both
opening directions, retained floor references and a nested 26-occurrence assembly.
Locating pins attach to semantic floor datums; independent expected positions
therefore test the entire lineage-to-face-frame-to-assembly chain.
Its 27-configuration signed-integral checks cover native and material moments,
while independent STEP/STL and browser checks cover exports, floor visibility and
native face selection. A required floor changed into a through-hole fails its
engineering obligation before publication. General interacting material removal
and manufacturing release gates remain unfinished.

### Coaxial stepped drilling and counterbores

A stepped-bore family represents each hole group as contiguous axial cylindrical
sections. A new aligned cylinder updates the maximum removed radius over its
axial interval, merges equal neighboring sections, and rejects unresolved radial
steps or axial gaps. These disjoint sections integrate the union of cuts exactly;
subtracting each complete tool volume would incorrectly count overlaps twice.
Both through-then-counterbore and blind-then-through orderings are supported,
along with counterbores from opposite caps and repeated radial steps.

Native reconstruction retains the base prism and replaces the hole boundary with
cylindrical walls and planar disk/annular shoulders. Recognition reconstructs and
compares canonical boundary topology, certifies support planes and clearances,
and derives properties without trusting construction metadata. A maximum-radius
through-hole surrogate certifies separation between hole groups and outer profile
boundaries. Display triangulation uses shared rings across walls, shoulders and
caps. The display representation never decides the Boolean result.

Stock face identity survives. Rebuilt hole faces use explicit geometric overlap
lineage: retained cylindrical support and axial interval, or retained coplanar
annular area with matching orientation. A wall can shorten or disappear;
a shoulder can change its inner radius or be deleted. Numeric entity indices are
not persistent design references. `bore_floor` identifies terminating tool planes;
`bore_shoulder` distinguishes annular termination. Planar datum queries use the
actual annular centroid and normal, enabling source-driven hardware attachment.

The bounded domain permits up to 32 separated hole groups and 128 total axial
sections. Cylindrical tools must open a stock cap; enclosed intervals, redundant
cutters wholly inside existing voids, noncoaxial intersection, uncertain coaxiality,
outer-profile contact and conical countersinks
remain unsupported. Classification uses a separate arithmetic envelope and never
snaps a dimensional offset into coaxial alignment.

`counterbored_fixture.star` integrates eighteen machining features and twelve
semantic shoulder datums into mirrored plates and a 32-occurrence fastening
stack. Independent 27-case signed-slice moments, material tensors and attachment
positions cover edits; STEP/STL oracles and browser selection cover downstream
consumers. A removed required shoulder or unsupported intersection rejects the
revision without replacing the prior published generation. This advances
fastening workflows while leaving general interacting cuts and manufacturing
release gates unfinished.

## Constraints and assemblies

Stage assembly-solver acceptance separately from today's attachment DAG: first a grounded
four-bar closed loop, then coupled mechanisms, then flexible occurrences. The
four-bar fixture must verify degrees of freedom, residual limits, contradictory
constraints, singular positions and solution-branch continuity under parameter
edits. Ordinary frame/joint solving must load zero exact bodies; collision checks
are separately scoped queries. A fast successful pose alone does not pass this
gate.

The current M1 attachment increment resolves explicit named part frames without
solving nonlinear constraints. Part definitions retain a map of local rigid
`datums`; an attached leaf instance retains its target occurrence, optional
descendant path, own datum name, target datum name, and local offset transform.
Evaluation uses:

```text
placement = target_placement × target_datum × offset × inverse(own_datum)
```

Frames default to identity, unit quaternions are validated rather than normalized,
and checked inverse/composition enforce the placement numerical envelope. Targets
are sibling parts or descendant parts inside child assembly occurrences. The
existing `instance` reference is the first occurrence ID; optional `target_path`
is a list of remaining literal IDs, ending at a part. No delimiter parsing,
ancestor traversal, definition-name lookup or global search is performed. A
nonempty path is an explicit schema-2 extension; older strict readers reject
the new field rather than resolve the wrong sibling. Empty paths serialize as
the original sibling form.

Preflight validates every declared path and datum, including unused definitions,
before geometry evaluation. Mixed part/module dependency cycles, chains beyond 64 occurrences
and paths beyond 64 components fail explicitly. Existing assembly depth,
expansion and work budgets also apply. Each reusable definition resolves once
bottom-up; indexed path lookup composes child occurrence transforms and the
terminal part’s resolved placement, including its own attachment. The result
is local to the containing definition, so two instances of a module can place
differently without changing its internal solution. Sources may be leaf parts
or whole rigid subassemblies. A subassembly attachment adds a required nonempty
`source_path` inside its referenced definition, ending at a part with the named
source datum. The source frame includes all resolved internal placements. Its
local placement is `target_frame × offset × inverse(internal_source_frame)`.
The solver does not alter internal placements or clone shared geometry.

Local dependency ordering spans both leaf and child occurrences. The first
target occurrence is the dependency even when a path descends further; this
rejects self-descendant targets and mixed part/module cycles. Child definitions
resolve before parent occurrences, and traversal uses resolved placements for
attached children on both source and target paths. `resolve_hierarchy_placements`
returns derived definitions with all attachments cleared, while the published
native hierarchy retains the original intent. Expansion and STEP use the
derived leaf **and child** transforms. The optional child `attachment` field
is another explicit schema-2 extension that old strict readers reject. Upward
references, flexible subassemblies and simultaneous mating remain future work.

Source hierarchy preserves attachment intent; flattened placements and STEP
occurrence transforms use the resolved result. Native validation recomputes and
compares derived placements. STEP exports placements, not the named datum or
attachment semantics. Datum-only edits change assembly identity and reuse exact
part geometry. `examples/datum_gantry.star` demonstrates attached posts, bridge,
clamps, and a rotated tool datum, then reuses the gantry in an inspection line.
Schema-2 face datums now associate a named frame with exactly one descendant of
an anchored feature role on the owning part's body. `face_datum` retains selector
intent, an explicit X-direction hint and a local offset in source. The kernel
uses the face's area centroid, outward Z normal and projected X hint to construct
a right-handed frame. A deleted/split face or ill-conditioned hint fails before
publication; no nearest-face or fallback-axis guess is made. Build reports retain
selection witnesses, source/target content hashes and the resolved frame. Existing
attachments consume these resolved frames without requiring a nonlinear solver.
`surface_mount.star` changes a ramp slope and moves/turns an attached plate;
independent slope equations check the plate's placement across parameter edits.
`instrumented_cartridge.star` exercises a sensor targeting a housing datum
inside a reused nested cartridge. General cross-boundary mating, joints,
multiple constraints, and DOF/residual solving remain separate future work.

The attachment acceptance corpus checks noncommuting own/target datum frames,
resolved targets with their own attachments, repeated child definitions, literal
punctuation in occurrence IDs, and invalid references in unused definitions.
The CLI test compares every published artifact byte-for-byte after rejected
path edits. The cartridge has 11 occurrences and seven shared part definitions;
spacing edits at 80, 100 and 120 mm recompute no solid geometry after the initial
build. Its independent material-volume formula is `84768 + 26414*pi` mm³.
`tools/preview-check/attachment-check.cjs` checks the attached mount and sensor
positions, orientations, picking and captured recipe in all three display clients.
The independent STEP oracle also resolves source attachment intent with matrix
arithmetic and compares imported local and world placements, hierarchy and
definition sharing. These checks establish this downward-reference contract,
not general mating, collision freedom or flexible subassembly solving.
`mounted_drive_module.star` adds two whole-module attachments with noncommuting
internal source frames and a sensor following an attached child. Its acceptance
tests check every cartridge leaf placement, geometry reuse across docking edits,
source-intent round trips, invalid source paths, mixed dependency cycles and
independent imported STEP child placements. Both examples share the same
physical volume formula but exercise different authored placement contracts.

The first constraint service is now `cad-sketch`, independent of the geometric
kernel. Named 2D point guesses and ten equation kinds—fixed, coincident,
horizontal, vertical, distance, equal length, parallel, perpendicular, directed
angle and circle-to-line tangency—feed a deterministic scaled/recentered local solve. Segment equations name
two endpoint-ID pairs. Directed angles are finite degrees in (-180, 180], from
first segment to second; parallel/perpendicular relations are unoriented modulo
π. Analytic
Jacobians drive damped least squares; pivoted QR assesses local rank and a
conditioning bound on the mixed normalized-linear/radian Jacobian; this bound
is not a world-coordinate error estimate. A successful profile requires each
linear residual within the stated millimetre tolerance and each angular residual
within the separate 1e-9-radian tolerance, zero local degrees of freedom, supported conditioning and
subsequent native polygon validity. Fixed/coincident errors use Euclidean norms.
Residuals and rank describe the returned world coordinates, including arithmetic
limits; unresolved precision fails explicitly. Schema-3 reports expose
`tolerance_mm`, `angular_tolerance_radians`, `max_residual_mm` (linear only) and
`max_angular_residual_radians`; per-constraint records emit exactly one of
`residual_mm` or `residual_radians`. Zero/numerically short segment directions
are unresolved, and angular arithmetic must satisfy its own precision gate.
Angular residuals within 1e-8 radians of the residual wrap cut fail explicitly;
trial steps use a conservative continuous-vector-rotation bound to prevent cut
crossing and return, including jumps invisible in endpoint residuals. Such steps
are damped, never accepted as silent branch changes. Global limits are 256 points and
1,024 constraints; every connected constraint island is limited to 32 points,
128 constraints and 256 iterations. At most 128 sketch features are demanded per
build. Components are preflighted before any numerical solve; unsupported large
connected systems return an island-specific resource error.

The typed `sketch` feature retains source guesses, named constraints and oriented
point-ID rings. Every definition receives structural validation, while only the
demanded dependency closure and global obligations are solved. Reports survive
memory/disk cache reuse; the solver fingerprint participates in profile hashes.
Success manifests pair `sketch_reports` with `feature_hashes`; failed solves retain
the input hash and numerical report without replacing the valid generation.
Starlark captures intent and the viewer inspects artifacts; neither performs
constraint solving. A textual recipe edit is the path to a revised solution.

This first solver reports local Jacobian DOF, not a proof of global freedom,
consistency or unique solutions. Initial guesses select a branch; unchanged
source is reproducible, but nearby edits do not yet have a branch-continuation
guarantee. Nonconvergence and residual witnesses are not a minimal conflicting
constraint set. Redundant equations can be consistent. Independent arc variables,
circle-to-circle tangency equations, sparse solving within large connected islands and assembly joints remain
target work. `constrained_slope_fixture.star` connects nonlinear sketch dimensions
to an extruded support, an associative inclined seat and a nine-occurrence
fixture. Independent three-side triangle formulas validate 27 configurations,
native moments, material mass and attached deck placement.

`constrained_angle_fixture.star` extends the corpus to an angle-driven right
triangle, an equal-leg trapezoid with parallel edges, and face-datum side-pad
attachments. Nine occurrences share five definitions. Its three parameters are
span 50..80 mm, incline 15..45 degrees and trapezoid top width 18..30 mm; defaults
are 60 mm, 30 degrees and 24 mm. Independent formulas use
`H = span*tan(theta)` and `D = sqrt(44²-((36-top_width)/2)²)`:
support volume is `30*span²*tan(theta)`, deck volume is `2*(36+top_width)*D`,
and assembly volume adds rounded-base volume `480*span+8736`, two pads totaling
160 mm³ and four bolts totaling `342π` mm³. Default transformed deck/pad bounds
remain within the base/support envelope `[0,0,0]..[80,80,6+60*tan(30°)]`.
The 27-configuration native suite and three-case independent STEP acceptance
pass, including centroid/full inertia and deck/pad placements. Browser inspection
passes three configurations in Canvas, embedded GPU and streaming GPU modes,
with selected-deck recipe inspection and no exact-body downloads. Angular
conflicts preserve the last valid published generation. These prescribed
attachments do not establish assembly-joint or load-solving capability.

Sketch profiles now accept circular through-holes defined by solved centre and
rim point references. The builder derives each radius from those returned points,
validates the native mixed polygon/circle profile before cache access, and
extrudes analytic cylindrical walls. Structural reference validation also covers
unused features; circle-handle identity participates in the feature hash.
Construction handles must be fully constrained, including their direction;
this representation does not add independent circle or tangency variables to
the solver. Polygon and circular holes share the existing 32-hole limit.
`constrained_drill_jig.star` exercises independently dimensioned hole spacing
and radii in a six-occurrence assembly with three shared definitions. Its guide
placements are prescribed from the declared dimensions, not associations to
solved cylindrical faces. Independent native tests cover 27 configurations,
plate central inertia and material mass/centre, and selective rebuild behavior.
Three independent STEP imports verify moments and six-occurrence hierarchy;
nine browser cases check 216 bore/guide-ring/plate rays and captured source.
A solved but geometrically invalid circle profile returns a structured feature
error and preserves every previously published artifact.

The circle-to-line tangency increment adds an explicit left/right side of an
infinite directed supporting line. A circle remains a centre/rim point pair;
its residual is signed line distance minus signed radius, in millimetres.
Analytic Jacobians accumulate shared endpoint contributions. Direction-error
amplification for distant contact is reserved in the linear convergence budget;
zero or numerically unresolved lines/radii fail explicitly. The solver does not
claim contact within the finite endpoint segment or global branch uniqueness.
Independent rotated/scaled/offset constructions, endpoint reversal with side
swap, shared-endpoint derivative checks and conflicting constraints cover this
contract. `tangent_bore_fixture.star` uses two opposed construction wedges to
locate bores in a seven-occurrence plate/feet/clamps assembly. Native acceptance
covers 27 configurations; separate STEP and browser oracles check moments,
hierarchy, bores and source identity. Circle-to-circle tangency, independent arc
variables and assembly contact solving remain separate work.

Curved sketch profiles connect solved point references to the native convex
line/arc path representation. `curve_outline` is mutually exclusive with polygon
loops; it names a start point and ordered line endpoints or arc centre/endpoints.
The final endpoint ID must equal the start ID. Structural checks cover all
features, while numerical solving and native profile validation follow demanded
outputs. Circular holes remain available, but polygon holes inside curved
outlines are outside the current path contract. Native arcs retain analytic
supports and shared endpoint topology through extrusion, transforms and STEP.
Unsupported winding, concavity, major arcs and radius inconsistency are explicit
failures, not a request to snap solved coordinates.

The sketch service exposes bounded `solve_refined` for this consumer. It retains
the original acceptance tolerance and all rank, conditioning and world-coordinate
precision checks, but continues accepted least-squares steps towards arithmetic
residuals. Only fully certified returned-world reports can become the retained
best snapshot. Stagnation or the iteration cap returns that snapshot with its
own iteration number; this does not certify stronger accuracy or guarantee that
native arc validation will succeed. Polygon-only profiles retain `solve`.
The kernel's arc-support consistency policy is unchanged. Independent capsule
moments and mixed-material assembly checks cover 27 dimension configurations;
fractional dimensions and asymmetric seed perturbations exercise compatibility
between the nonlinear solve and native curved geometry.

Constraint islands now follow actual constraint/point incidence, independent of
profile ring and circle membership. Fixed constraints are unary; shared point
references connect their equations. Unreferenced free points remain singleton
islands rather than disappearing. Islands and their membership are ordered by
stable source IDs, with independent recentering/scaling and complete per-group
reports. Aggregation sums local rank, DOF, redundant equations and snapshot
iterations, takes residual maxima and the worst available local condition bound,
and preserves the most severe failure status. No successful neighboring group
can mask numerical failure, nonconvergence or unconstrained motion. Schema-3
reports preserve each local witness; recipe IR stays at schema 2. This is bounded
dense solving of disconnected components, not sparse elimination inside a large
connected component or automatic removal of fixed anchors from incidence.
`modular_bore_plate.star` validates 64 points in 34 islands, 30 analytic bores
and a 33-occurrence assembly with three shared definitions. Independent native
array integrals cover 27 parameter combinations and selective regeneration.
Three independent STEP imports verify moments/hierarchy, and nine browser cases
check 2,160 bore/guide/plate rays with zero exact downloads. A single local full
release publication measured 3.359 s and 46,484 KiB process peak RSS; [the raw
record](benchmarks/islands-modular-bore-local.json) pins binary/source identities,
workload, environment and measurement scope. This includes every export and
preview variant, not browser interaction. Code review identifies nine repeated
default plate tessellations across those consumers, plus two coarse levels;
a generation-scoped certified mesh cache is a concrete remaining optimization.

Evolve this constraint service separately from geometric construction. The target 2D sketch
solver reports degrees of freedom, residuals, conflicting constraints, multiple
solution branches, and convergence limits. Preserve branch/continuation intent
across small parameter edits; diagnose underconstrained sketches explicitly.

The assembly solver operates on rigid frames, named attachment datums, joint
parameters, and sparse constraints. It must not need full solid data to solve
ordinary placement. Partition independent constraint islands, fix gauge freedoms,
use deterministic ordering and bounded nonlinear solving, and report residuals
and unresolved degrees of freedom. Collision and clearance queries are separate
from kinematic constraint satisfaction.

A part definition references immutable geometry and display assets. An instance
has its own persistent identity, definition/version reference, rigid transform,
configuration, suppression state, material/metadata overrides, and parent path.
Repeated fasteners share one asset. Nested assemblies need cycle detection,
transform composition, instance-aware picking, and distinct BOM quantity rules.

Distinguish rigid subassemblies from flexible, instance-solved subassemblies.
A rigid definition reuses one internal placement solution. A flexible occurrence
owns its configuration and joint state, allowing two occurrences to share part
geometry while having different internal placements. Solving one occurrence
must not mutate the shared definition or another occurrence. Cross-boundary
constraints join the affected solver islands explicitly; BOM identity and
geometry reuse remain independent of the resulting motion state.

Load bounds, transforms, coarse graphics, names, and required datums first.
Request exact bodies only for affected modeling, certified queries, precise
sections/drawings, or export. Track independent budgets and eviction for exact
bodies, CPU display assets, GPU residency, and queued work. A many-part assembly
must be useful with zero resident exact B-reps during graphics-only inspection.

### Demand-driven regeneration and configurations

Graphics-only loading and selective regeneration are separate requirements.
The target builder evaluates the dependency closure of requested outputs and
validation policies, reuses verified immutable results, and loads exact bodies
only for operations that require them. Bound exact-body residency separately
from display caches; allow eviction and recomputation without losing provenance.
The local builder now supports explicit output selection while retaining eager
`build()` and full `check` behavior. It validates the whole recipe structurally,
then evaluates selected body/assembly dependencies plus every global assertion
and face-datum dependency. Reports list requested outputs, evaluated/skipped
features and validation roots. Unknown outputs fail before geometry evaluation;
selected assemblies and globally asserted assemblies are materialized. An
assertion-only assembly is retained only for validation and is not published.
Published source stays complete,
and selected publication replaces a complete selected generation rather than
merging artifacts from different scopes.

This is dependency-based execution, not yet bounded exact-body residency:
evaluated intermediates and the engine's memory cache remain resident. Source
compilation, structural validation and global obligations still have full-recipe
cost. Benchmark a one-part edit inside a large assembly as well as opening its
viewer. `workcell_line.star` provides a separately requested service station;
independent tests cover unselected edits, one-tool regeneration and a 1,000-station
recipe with 40 unique tool variants.

A configuration identity must include the pinned definition revision, resolved
parameters, suppression rules and material overrides. In the target model,
suppression removes an occurrence subtree from that configuration's evaluated
assembly, constraints, geometry queries, mass rollups, BOM quantities and
manufacturing outputs. References to suppressed occurrences fail explicitly
unless their dependent features are also suppressed; they cannot use stale
placements. An all-configurations inventory may list them as suppressed, with
no quantity contribution to the active release. Record these participation rules
and their schema version in query and release identities. Viewer hiding only
changes inspection visibility and never changes engineering participation.
Cross-level references use explicit instance paths and revision scope. Their
dependency cycles are checked independently of cyclic mechanical constraints:
a nonlinear solver is not a way to resolve a cyclic feature build.

## Display assets, streaming, and GPU responsibilities

### Independent display build

Tessellation is a shared compilation product. Within a publication job, preview,
display manifests and STL exports request immutable certified meshes through
`cad-mesh::GenerationMeshCache`. Legacy library entry points retain their signatures
and allocate their own cache; callers can opt into explicit shared-cache variants.
STEP and native engineering queries continue to consume authoritative geometry.
This boundary applies before introducing distributed workers or cluster LOD.

The cache identity includes verified geometric content, native numerical policy,
the full tessellation policy and mesher implementation identity. A caller's
claimed geometry hash is not sufficient evidence for reuse. Keep occurrence
placement, materials and captured recipe provenance outside shared mesh identity;
bind them separately to the published generation. Retain native face/edge mapping
and error evidence in the immutable mesh result. Cache successful certified
results only; a cache hit must not bypass validation of changed source geometry.

Acceptance compares cached and uncached geometry, winding, semantic picking and
exported measurements. Changing geometry, tolerance or meshing policy must select
a different entry; identical geometry used by multiple outputs should mesh once
per policy. Measure this construction cost separately from viewer frame time.
The CLI now shares one cache across body and assembly publication, including
separate analytic LOD policies. Analytic meshes and nonconvex/holed planar-face
triangulations are cached; inexpensive convex planar fans remain local to writers.
Native validation stays in the exporter and assembly consumers. STEP remains on
its native path and may repeat planar triangulation independently.

Retention defaults to 64 MiB and at most 1,024 entries with deterministic FIFO
eviction. Charges cover serialized native witnesses, mesh vector capacities, Arc
headers and entry descriptors, not allocator/tree overhead, construction scratch,
output strings or caller-held meshes. Oversized successful meshes return uncached;
zero-budget mode provides an uncached reference. This is a retention bound, not a
peak process-memory guarantee. Manifest counters expose hits, misses, successful
builds, failures, evictions and retained charges with kernel/mesher identities.
Budgeted and cancellable preprocessing remains separate target work.

The [local shared-mesh measurement](benchmarks/shared-mesh-modular-bore-local.json)
records three fresh-process full publications of the 30-bore plate at
0.77–0.85 seconds, compared with the earlier single 3.36-second observation.
Each publication reports six constructions (two analytic definitions at three
policies) and thirteen hits, with 2,166,720 charged retained bytes. Peak child RSS
across these runs was 47,672 KiB. STEP, STL and SVG for both body and assembly
match the earlier baseline byte-for-byte. Shared-host timing and uncontrolled OS
caches make this performance evidence local, not a hardware release gate.

Derive meshes in a separate worker. A display cache key includes geometric
content, mesher implementation, chordal/angular error, edge policy, coordinate
quantization, and display-format version. Preserve shared boundaries, normals,
sharp feature edges, and triangle-to-semantic-face mappings during tessellation.
Record measured or conservative error bounds; do not assume a simplifier's
heuristic score is automatically a certified distance bound. Specify the metric
and direction (for example, a conservative symmetric surface-distance bound),
including tessellation, simplification and quantization contributions. Test thin
features and boundary preservation separately from a scalar distance metric.

The error ledger must also include any accepted discrepancy between native
vertices and their supporting curves/surfaces, shared-boundary reconciliation,
coordinate rebasing/conversion, and GPU arithmetic. Distinguish an asset's
object-space bound from the additional error of placing and displaying it.
Reserve these contributions before spending the remaining tessellation budget;
if that budget is exhausted, report an unattainable policy rather than claiming
the requested accuracy. Acceptance includes small features at large offsets and
valid native endpoint/support discrepancies. Retaining native endpoints must
not silently invalidate a chord-error bound.

First implement cached meshes, instancing, frustum culling, coarse part LODs, and
explicit residency budgets. Then add bounded clusters/meshlets, hierarchy
construction, crack-safe transitions, compression, spatial indexing, and
screen-space-error refinement. Use measurements to select cluster sizes rather
than hard-coding a universal 64–128-triangle rule.

Do not merge away part identity, topology labels, sharp boundaries, or thin
engineering features merely to reduce draw calls. Selected entities, dimensions,
and critical edges require refinement or a separate precise overlay. A coarse
cluster can represent several faces: coarse picking returns an instance/region
candidate and refines before claiming a particular semantic face.

### Target display hierarchy contract

Implement this contract in independently reviewable stages, retaining the current
portable and WebGL2 inspection paths until a replacement passes their identity
and failure tests. Each stage requires an end-to-end fixture, not just an asset
encoder or a successful screenshot:

| Stage | Deliverable | Gate before proceeding |
| --- | --- | --- |
| M3a: part LOD | Shared coarse/fine meshes with conservative accumulated error and semantic mapping | Curved and thin-feature fixtures meet a declared 1-pixel target along a fixed camera trace, or visibly report unmet detail; picking and captured source remain generation-correct |
| M3b: metadata paging | Paged occurrence/spatial hierarchy with bounded decoding and assembly proxies | First meaningful coverage does not expand all occurrences; a 100k-occurrence fixture stays within declared metadata and request budgets during rapid camera reversals |
| M3c: cluster streaming | Shared part cluster hierarchies, crack-safe replacement and budgeted page residency | Partial/corrupt/out-of-order children preserve parent coverage; measured error and ownership survive refinement and eviction |
| M3d: accelerated visibility | Conservative occlusion and optional GPU traversal | Compare visibility against an unculled reference across disocclusion, sections and selected objects; retain the path only with measured gains on supported hardware |

The 1-pixel target is a proposed test configuration, not a claim about current
rendering. Pin viewport, projection, trace, geometry and error metric; record
achieved error and time to requested detail alongside frame times. Coarse
coverage cannot count as detail completion. Establish detail-latency thresholds
from the declared storage/network profile before accepting the stage. Cluster
size and mesh-shader support are implementation choices, not product promises.

The planned hierarchy is a derived, independently versioned artifact. It must
not become the design database or introduce a dependency from the kernel to a
particular renderer. Its minimum records are:

| Record | Required fields and contract |
| --- | --- |
| Part display manifest | Exact geometry hash, mesher/version and tolerance policy, local coordinate frame, root page references, semantic mapping version |
| Cluster/page descriptor | Content digest and encoded/decoded sizes, conservative bounds, object-space geometric error, refinement dependencies and required boundary data |
| Assembly spatial node | Conservative occurrence bounds, child nodes, instance paths and transforms; reusable part hierarchies remain shared |
| Selection mapping | Source generation, occurrence path and candidate semantic face set; a coarse triangle is not necessarily one exact face |
| Residency record | Requested/resident/failed state, reserved bytes, last use, fallback dependencies and cancellation generation |

Convert conservative object-space error to a screen-space bound using the
camera and occurrence transform. Refine when that bound exceeds the configured
pixel budget, with hysteresis to avoid repeated swaps around a threshold.
Near-plane cases require conservative treatment. A parent remains visible until
all children needed for a crack-safe replacement are ready. If residency limits
prevent refinement, expose the reduced detail rather than claiming the error
target was met. Small screen coverage alone cannot justify dropping a thin part
or an engineering-critical overlay.

Distinguish **part cluster LOD** from **assembly HLOD**. Part clusters refine
regions inside one shared geometric definition. Assembly HLOD may aggregate
many occurrences for distant coverage, but must retain a generation-pinned
mapping to the represented occurrence subtree. An aggregate cannot impersonate
one part or face. Picking it requests display/metadata refinement and exposes
unresolved selection until ownership is known; it never evaluates a recipe.
Hiding, isolating, selecting or sectioning a member must invalidate any aggregate
that cannot express the requested visibility correctly. Keep a conservative
proxy while its replacement loads. Materials, transparency and critical-edge
policies also constrain aggregation; reducing draw calls cannot override them.

A cluster hierarchy can be a DAG with shared boundary/replacement dependencies,
not merely a tree in which each child can be swapped independently. Define the
active refinement frontier as complete surface coverage with no duplicate
parent/child coverage and compatible shared boundaries. Test mixed refinement
levels across adjacent faces, shared pages and different instances of one part,
including cancellation and eviction halfway through a replacement. This is a
derived display graph, separate from the feature, topology and assembly graphs.
Validate acyclic, bounded refinement dependencies before traversal; malformed
cycles or shared-child ownership cannot cause unbounded work or duplicate picks.

Refinement demand belongs to an occurrence and inspection view, while page
storage belongs to the shared part asset. The target scheduler permits different
active frontiers for near and distant occurrences without duplicating immutable
pages. Combine all live demands when prioritizing requests and retaining fallback
dependencies: cancelling one demand must not cancel or evict a page still needed
by another. Test two instances of the same part at different projected scales,
selection changes, multiple views, and cancellation during shared fetches.
Today's whole-part LOD shares one level across a part's occurrences; it does not
yet implement this finer-grained demand ownership contract.

Maintain separate occurrence and part-level spatial hierarchies. Start with
frustum traversal; add conservative occlusion only after tests cover disocclusion,
camera changes, selected objects and stale depth. Unique geometry, depth
complexity and request latency still affect cost: this architecture does not
promise constant work for arbitrary assemblies.

Occlusion evidence must account for the displayed approximation's displacement
and quantization bounds. A coarse mesh that closes a real opening, or a bounding
proxy, cannot be treated as an exact opaque occluder. Cull only with conservative
evidence; otherwise retain the potentially visible geometry. Bind that evidence
to the camera, clipping/section planes, visibility configuration, display level
and source generation, and invalidate it when those inputs change. Acceptance
includes views through thin openings and newly exposed section interiors,
compared with an unculled reference.

Page assembly metadata and spatial nodes as well as mesh data. A root manifest
must not require expanding every nested occurrence before first coverage.
Descriptors declare bounded decoded metadata sizes; page traversal preserves
stable occurrence paths and configuration identity. When even all part-level
root meshes exceed residency limits, retain assembly-level conservative proxies
with explicit unresolved detail and ownership. Measure first meaningful coverage
separately from full overview coverage and requested-detail completion.

Benchmark a fixed camera trace with repeated parts, many unique parts, thin
features, deeply nested instances, rapid camera reversals and a constrained
network. Record first coarse coverage, time to requested detail, frame-time
percentiles, page churn, transferred bytes, separate CPU/GPU memory accounting,
and picking correctness during refinement. Include context loss, corrupt pages,
out-of-order delivery and budget exhaustion. Hardware and error budgets belong
in each result, alongside the workload and implementation revision.

### Streaming and cache contract

Keep an immutable validated design generation separate from its derived display
revisions. A later completed detail level publishes a new immutable display
manifest referencing the same exact-geometry and source-generation digests.
Only advance the display pointer after its required assets pass validation;
do not rewrite engineering results or the design release. Camera demand may
mix compatible levels within that pinned generation, but never geometry from a
different source revision. Failed refinement leaves the previous complete
display manifest available. Race tests must cover design changes arriving while
an older generation's refinement job finishes. This is the target progressive
publication contract, not a claim of asynchronous refinement in today's server.

An immutable assembly manifest references versioned part manifests and display
pages. Serve content-addressed blobs with range-capable HTTP where useful;
transport is replaceable and does not require WebSockets. Small generation
notifications can be separate from bulk geometry transfer. Prioritize visible
coarse coverage before refinement, cancel obsolete requests, apply backpressure,
and keep parent LODs resident until complete replacements arrive.

Each response and displayed generation carries its own identity. Never combine
new metadata with old geometry or accept an A→B→A interleaving as a consistent
snapshot. Missing assets produce a visible loading/proxy state; they do not mean
empty geometry. Cache eviction must not discard the only visible fallback. Reserve decoded CPU
bytes, upload staging and resident GPU allocations separately; release them only
when decoding work and GPU use have completed. A digest validates bytes, not the
allocation sizes claimed by an untrusted descriptor: validate decoded counts and
indices before allocating or uploading.

Budget display **construction and storage**, as well as runtime residency.
Meshing and hierarchy workers declare time, peak memory, temporary disk and
output-byte limits. Record hierarchy bytes relative to the finest mesh, build
time per unique definition and deduplication across instances. A placement-only
edit reuses part hierarchies; a geometry edit invalidates that definition's
display artifacts and affected assembly bounds/aggregates without rebuilding
unrelated parts. An interrupted display build cannot replace the last complete
manifest with partial pages. Benchmark cold cache, single-definition edits and
cache eviction: cheap opening achieved through unbounded preprocessing or disk
growth does not pass the large-assembly gate.

Exercise construction budgets with a single highly detailed unique part as well
as many repeated parts. Meshing, simplification, clustering and compression must
observe cancellation and bounded work independently. Publish a display level
only when its required pages and validation evidence are complete; a failed or
cancelled finer level retains the last valid coverage. Partial refinement output
must never masquerade as a complete level or invalidate a released generation.

### GPU and portability

GPU acceleration is a replaceable execution choice, not a second source of
geometric truth. Keep these responsibilities explicit:

| Work | Target execution and authority | Acceptance boundary |
| --- | --- | --- |
| Modeling, topology and semantic history | Headless Rust kernel; CPU reference path required | Same supported-domain and numerical contracts with no graphics device present |
| Constraints and engineering queries | Headless solver/query workers; optional acceleration behind their interfaces | Independently validate residuals, bounds and witnesses; a device failure cannot produce a successful partial result |
| Meshing and hierarchy construction | Budgeted derived-asset workers; CPU or GPU implementations | Record implementation and error policy; certify boundaries and semantic mapping before publication |
| Visibility, refinement, rasterization and display picking | Inspection renderer; portable baseline plus optional accelerated paths | Preserve occurrence/source identity, conservative coverage and explicit unresolved detail |

An acceleration experiment must specify its reproducibility envelope before
sharing caches with another backend. Bitwise-identical results and equivalent
validated engineering results are different claims. Record backend and numerical
policy in artifact identity whenever they can affect output; never reuse an
uncertified result merely because the source recipe hash matches.

Direct analytic GPU rendering is an optional display experiment for supported
trimmed surfaces. It must handle trim boundaries, seams, depth ordering, section
planes and selection ownership, and quantify device-precision error. Rendering
a smooth cylinder alone does not establish those contracts. Keep a certified
mesh fallback for unsupported surfaces or devices. Analytic pixels do not become
STEP geometry, manufacturing measurements, or evidence that a Boolean succeeded.
Compare this path with clustered meshes on the same camera trace, fidelity and
hardware before adopting it; it is not a prerequisite for the commercial workflow.

Target a Rust graphics backend such as `wgpu` with browser WebGPU and native
backends, subject to a supported-device matrix. Baseline rendering must not
require mesh shaders: indexed instanced rendering plus optional compute culling
can support the first scalable implementation. Add mesh-shader or other advanced
paths only behind runtime feature checks and measured gains. Keep a lower-capacity
fallback for unsupported devices; the current Canvas viewer is that prototype,
not the planned large-assembly renderer.

The first GPU inspection backend is implemented as an opt-in, dependency-free
WebGL2 client generated by Rust. It uploads one triangulated local mesh per shared
content hash and draws occurrences with hardware instancing. It retains local
face IDs and occurrence identity in depth-tested picking. Mesh-local recentering
and camera-relative instance translations reduce float32 coordinate loss; inputs
outside its display range fail explicitly. This is approximate inspection, with
no change to authoritative kernel coordinates or engineering measurements.

Normal assembly builds include both `.preview.html` (Canvas) and `.gpu.html`.
`build/watch --display-only` can publish GPU assembly inspection without exact
body files, STEP/STL, or SVG generation. Opening that artifact loads zero exact
B-reps. The headless build still evaluates and validates geometry. Assembly builds also publish `.stream.html` and schema-2 `.stream.json`, with
separate `mesh-<sha256>.display.json` assets. The streaming page embeds metadata,
bounds, occurrence paths and captured recipe context, then fetches visible meshes.
Descriptors record exact byte length, SHA-256, bounds and geometry counts. The
client bounds response reads, verifies the digest before parsing/upload, and
rejects stale request completions. Four concurrent requests, conservative
orthographic frustum culling and LRU eviction bound the working set. Selected
proxy instances take priority for refinement. Loading, failed or over-budget
parts remain visible bounds proxies; proxy picking never claims an exact face.

The default 64 MiB geometry accounting budget charges encoded payload bytes plus
geometry VBO bytes, including reservations for in-flight requests. It is not a
hard JavaScript heap or total GPU-memory limit: parse transients, instance buffers,
picking targets and driver allocations are separate. Repeated occurrences share
one resident mesh per geometry hash. Distinct hashes with identical payloads can
still have separate resident VBOs. Visibility uses a deterministic CPU occurrence BVH over conservative world
bounds. Internal nodes reject off-screen groups; leaves retain the individual
oriented-bound visibility test. Accepted occurrence indices are sorted into
source order, preserving picking IDs and draw order. The hierarchy is built once
from display metadata and shared by embedded and streamed GPU views. Per-camera
node and occurrence test counts expose pruning separately from frame timing.

This first hierarchy still loads and indexes all occurrence metadata at startup.
It does not page metadata, index triangles inside a part, perform occlusion
culling, or generate meshlets. Those remain target work; overview
views may still visit every occurrence. A hierarchy over display bounds is not
an authoritative interference or clearance query index.
The embedded `.gpu.html` remains available and also uses frustum culling.

The first M3a increment adds whole-part LOD for supported analytic geometry.
Schema-2 streaming manifests retain `mesh_assets` as the finest compatibility
representation and add `mesh_lods`, with up to three coarse-to-fine descriptors
per geometric hash. Native tessellation policies request 0.5, 0.1 and 0.01 mm;
each descriptor carries its actual `geometric_error_mm`, bound to the verified
payload's tessellation error. Planar parts have zero chord approximation and one
asset. Identical payloads deduplicate. Native bounds and semantic face IDs remain
invariant; generation is bounded at 150,000 faces and 600,000 face points across
levels, in addition to the existing finest-level limits.

The orthographic viewer shares one active level across occurrences of a part.
It bootstraps coarse coverage, converts geometric error to physical framebuffer
pixels and adds conservative CPU/float32 arithmetic allowances. Refinement
targets one pixel, with a 0.8-pixel downgrade threshold to reduce oscillation.
This is a geometric detail policy, not certification of raster coverage, depth
quantization or the complete rendered image. All-planar scenes retain the
single-level streaming path; they make no adaptive-detail claim.

Refinement reserves both the current fallback and incoming level. Only after
hash verification, error/count/bounds validation and upload does a replacement
take over. If the requested level cannot fit, try an affordable intermediate;
otherwise retain the current surface and report unmet detail. An explicit budget
reduction below current residency can require proxies. Request cancellation
rejects obsolete completions, and blocked refinement settles without waiting
forever for an unattainable allocation. Finest-detail limits and numerical
allowances can also leave the target unmet at extreme zoom. No display request
loads exact bodies or recomputes engineering evidence.

Independent radial-deviation tests cover circle and conical meshes; browser
acceptance covers annular cap/bore picking, repeated-part sharing, delayed,
missing and obsolete refinement, intermediate budgets and forged error metadata.
An independent thin-annulus fixture retains a 0.2 mm wall gap, all four semantic
faces, shared watertight triangle edges and the expected polygon-annulus volume
at every level. None of those display volumes replaces the native volume.
`spindle_line.star` combines tapered housing variants and shared drilled bases,
shafts and rings. This is bounded whole-part refinement; metadata paging,
cluster transitions, full screen-space error certification and hardware scale
gates remain open.

Streaming requires HTTP(S), WebGL2 and Web Crypto (localhost is supported).
Content-addressed assets are verified individually; unavailable assets remain
explicit proxies. The GPU clients require a reload after rebuilding; Canvas
retains automatic generation refresh. Old open pages whose assets were removed
must reload the published generation; the current local publisher is not a
multi-generation retention service.

GPU publication permits up to 100,000 occurrences, with at most 50,000 unique
faces and 200,000 unique face points. These are resource ceilings, not measured
interactive performance guarantees. Picking targets are capped at four million
pixels. Explicit errors cover unsupported WebGL2, shader/framebuffer failures,
numeric range, and context loss. Portable previews remain available for normal
builds within their lower limits. The benchmark harness records renderer,
hardware, workload, buffers, draw calls, first view and synchronous frame timing;
software-rendered measurements do not satisfy the discrete-GPU release gate.

GPUs are natural for visibility, rasterization, display refinement, batch spatial
candidates, and some tessellation work. They do not automatically solve robust
trimmed-surface intersections, topology reconstruction, or persistent naming.
Use object-local or camera-relative coordinates for GPU display to avoid losing
small features in large world coordinates; keep authoritative coordinates and
unit transforms independently. Authoritative modeling initially uses CPU algorithms with deterministic scheduling
and documented floating-point limits. Evaluate GPU geometry kernels separately
against the same error and validity contracts.

Analytic surface rendering is an optional research path. Trimming, seams,
precision, clipping, picking, and interoperable exported geometry still need
solutions. Neither one-triangle-per-pixel nor infinitely smooth zoom is an
unconditional performance/correctness guarantee for engineering assemblies.

## Queries, drawings, and engineering deliverables

Expose accuracy explicitly: visual approximation, conservative bound, or
validated geometry result with tolerance/error. Bounding boxes can reject
collision candidates; overlapping boxes cannot prove collision. Approximate
LOD meshes cannot certify clearance or machining dimensions. Exact query workers
load only candidate bodies and attach evidence to the result.

Bind each query to an immutable generation, configuration, occurrence scope,
coordinate frame, accuracy policy and implementation version. Return witnesses
and an explicit complete, indeterminate, unsupported or cancelled status. A
negative interference result requires complete conservative candidate traversal;
missing bodies or exhausted budgets cannot become a claim of no interference.
Keep authoritative geometry bounds separate from display bounds expanded for
tessellation/quantization error and from camera-dependent screen bounds. Display
simplification must never shrink the bounds used to reject engineering candidates.

The native mass-property query now integrates volume, volume centroid and central
volume inertia directly from validated planar B-rep boundaries. Assembly queries
reuse definition properties, rotate tensors into the root frame and apply the
parallel-axis theorem over occurrences. Mass weighting uses explicit part density;
missing material makes mass, center of mass and mass inertia unknown, without
hiding the available geometric properties. As in the BOM, overlapping occurrences
are summed and empty occurrences still require material for complete mass status.

`cad properties` is read-only with respect to source and published outputs. Its
versioned JSON records units, coordinate frame, tensor sign convention, source,
parameters and implementation identities. Body density is an explicit query
input; assembly density comes from part metadata. Local-reference integration,
compensated accumulation and a centered second pass have numerical range and
cancellation guards. These are checked floating-point engineering results, not
claims of symbolic precision. Independent box/prism formulas, mixed-material
balance-arm fixtures and optional OCCT centroid/tensor comparisons verify them.
Principal axes, uncertainty propagation and dynamics remain future work.

Introduce a versioned drawing/annotation IR containing view definitions,
projection/section intent, semantic references, dimensions, tolerance standards,
units and revision identity. Rendering consumes that IR; it does not infer
manufacturing intent from mesh edges. Annotation regeneration must preserve its
reference or report deletion/ambiguity, and stale drawings cannot be presented as
current released deliverables. Import validation and round-trip loss reporting
need an adversarial corpus before broad format support is declared.

Treat manufacturing output as a first-class domain: associated orthographic and
section views, hidden-line removal, dimensions and datums, GD&T/PMI intent,
materials/density, mass properties and inertia, BOM, part/revision numbers, and
configuration-aware quantities. Drawings and annotations have stable design
references and testable revision behavior, not screenshot-only geometry. Manufacturing datum systems and tolerance semantics
are distinct from the rigid attachment frames currently named `datums`.

For a declared frame/fabrication workflow, add configuration-aware cut lists as
derived manufacturing artifacts: stock profile, material, finished length, end
treatments, quantity and source occurrence/revision. Group only manufacturing-
equivalent members; equal display meshes alone do not establish equivalence.
Source must declare stock allowances and process assumptions. Acceptance varies
member length, material and suppression, then checks regrouping and quantities
against the revised assembly. This is planned scope; the current BOM does not
constitute a fabrication cut list or a stock-nesting optimizer.

Evolve today's planar STEP/STL export toward analytic STEP import/export and
assembly/PMI-capable AP242 subsets, with a published support matrix and round-trip
loss reports. Add useful manufacturing and display formats through adapters;
open-format support must state which entities, units, transforms, and metadata
survive. glTF or mesh packages serve visualization; STL does not preserve CAD
semantics. BOM/PLM/ERP adapters consume versioned identifiers and reports.

A release policy declares required manufacturing semantics separately from
advisory export losses. Losing a required unit, tolerance, datum reference,
revision or occurrence mapping blocks that release even if the exported solid
is geometrically valid. Independent round-trip checks compare these semantics
as well as shape validity; successful display in another application is not
manufacturing acceptance.

## Reproducibility, collaboration, and operations

### Resource ownership across the product

The target runtime assigns each allocation and outstanding job to an explicit
owner. Configure separate budgets rather than treating the viewer's geometry
cache limit as a product-wide memory guarantee:

| Owner | Budgeted resources | Behavior at the limit |
| --- | --- | --- |
| Build scheduler and exact-body store | Resident exact bodies, operation scratch, queued jobs and temporary artifacts | Spill or recompute only verified immutable intermediates; otherwise return a resource failure and retain the prior generation |
| Constraint/query worker | Problem size, factorization/candidate scratch and iteration/work limits | Return unresolved or failed obligations with diagnostics; never certify an incomplete solve or negative interference result |
| Display compiler | Tessellation/hierarchy scratch, output bytes and temporary disk | Retain complete published coverage; incomplete refinement is not a valid asset |
| Inspection client | Decoded metadata, CPU meshes, request/decode staging and GPU allocations | Defer refinement, retain a valid proxy/parent, and expose unmet detail |
| Artifact service | Disk retention, transfer concurrency and active-job/release pins | Apply backpressure and collect only unreferenced artifacts; retained releases remain reproducible |

Every job reserves its working budget before starting expensive work and releases
it on completion, failure or cancellation. Shared allocations are charged once
to their storage owner while each consumer retains a demand/pin; cancelling one
consumer cannot release another's live dependency. Add process-level peak-memory
measurements and simultaneous build/inspection/export workloads to acceptance:
component counters alone omit allocator overhead and overlapping scratch space.
These are target contracts. Current generation-cache accounting and display
limits cover only their documented scopes.

### Revision and release lifecycle

Published-generation integrity is the first implemented boundary toward release
evidence. Build manifests have a separate `manifest_schema_version` and a BLAKE3
digest/byte-length inventory of every payload, excluding the manifest itself.
`cad verify` checks the exact directory membership and payload bytes without
recipe evaluation. Its returned manifest digest can be pinned by an external
release record and supplied as `--manifest-hash` on later verification. This
detects changes relative to that pin; self-consistent manifests alone do not
authenticate a publisher or prove engineering correctness. The planned evidence
ledger must additionally bind executed checks, independent oracles, build/source
identity and their results. Artifact integrity does not close those obligations.

Hosted project authorization applies to source snapshots, manifests, meshes,
reports and every referenced dependency. Knowing a content digest grants no
access. Deduplication is a storage optimization, not permission inheritance:
cross-project sharing cannot expose another project's geometry, recipe text or
artifact-existence information. Authorize reads and job inputs in project scope,
and bound delegated download access to that scope and lifetime. The present local
artifact server is not an authenticated multi-tenant service. Gate hosted M7
delivery with cross-project isolation and revoked-access tests.

Keep source in Git and large immutable artifacts in a content-addressed store.
Do not require binary geometry merges. Semantic diffs should explain changed
parameters, resolved selections, topology/geometry, mass, clearances, and BOM.
Git handles textual collaboration; an engineering revision adds a pinned build,
validation results, approval state, and auditable change provenance. Distributed
workers need cancellation, resource quotas, crash recovery, atomic publication,
and bounded concurrency before remote multi-user hosting is offered.

Source merges require semantic validation after Git's textual merge succeeds.
Two individually valid edits may jointly invalidate a fit, face reference or
assembly constraint. Compare the merged build with both parent revisions and
report changed obligations; never resolve that conflict by choosing whichever
geometry artifact was written last. Publication uses an expected base revision
and an atomic update of the release pointer. Concurrent stale publishers return
a conflict while their immutable artifacts remain available for review.

Keep stable part numbers distinct from content hashes, source revisions and
occurrence paths. A release manifest pins source and dependency locks, effective
configuration, kernel and policy versions, validation evidence, drawings, BOM
and exchange files by digest. Adapters for PDM/PLM/ERP consume that manifest;
they cannot silently change authoritative recipes. Optional locks may coordinate
non-mergeable imported assets or organizational release policy, but ordinary
recipe authoring remains compatible with Git branches and offline work.

Where a team requires release approval, bind it to the precise source,
configuration, build and validation digests; a changed input requires a new
approval. Released manifests pin the complete transitive dependency and evidence
graph, including imported assets. Garbage collection preserves everything
reachable from retained releases and active jobs. Schema or kernel migrations
create new artifacts and explicit provenance links, preserving the original
release and its evidence rather than rewriting its history.

AI acceptance evidence must cover an agent proposing a source patch, receiving a
structured geometry or reference failure, and producing a corrected revision
through the public headless interface. Replay the accepted source with no model
service and reproduce its declared engineering results. Preserve provenance of
the proposal separately from deterministic evaluation: model output itself is
not promised to be repeatable and cannot serve as a geometric correctness oracle.

Version operation schemas, kernel semantics, solver policy, meshing policy,
serializer formats, source dependencies, and build toolchains independently.
Reuse unaffected DAG nodes. Native recipe artifacts, imported exact geometry,
display meshes, and engineering reports have different invalidation rules.
Cross-platform byte identity is a tested capability where achieved, not a blanket
promise for all floating-point computations or GPU executions.

## Verification and scale gates

Expected-output tests must include volume/area/bounds and topology, not just
images or equality with the implementation's own calculations. Use independent
analytic constructions, metamorphic identities, randomized/adversarial inputs,
edit/regeneration sequences, differential checks against independent kernels,
and round-trip exports. An oracle disagreement is evidence to investigate;
neither majority vote nor one successful import proves correctness.

Keep property fuzzing, minimization, numerical-policy boundary tests, mutation
validation, cancellation/resource tests, and source/manifest publication races
in the test strategy. Benchmark both cold and warm caches, many repeated parts
and many unique parts, malformed imports, slow networks, and device loss.

### Commercial workflow benchmark protocol

Maintain versioned engineering tasks for modeling, revision/regeneration,
assembly solving, inspection, exact queries and manufacturing release. Record
successful obligations, unsupported operations, incorrect results and resource
failures separately before comparing latency. A fast failed operation or a
coarse preview is not a completed engineering task. Pin source, configuration,
input geometry, numerical policy and expected design intent for each task;
record cold/warm timings and peak resources for each stage independently.
Retain every task in the reported corpus denominator, including unsupported and
failed runs; report timeout and resource exhaustion explicitly.

Any comparison with a commercial product must identify its version and use the
same engineering task, output fidelity and comparable hardware. Document needed
manual repairs, altered tolerances and unavailable capabilities, and check
outputs against the same independent acceptance oracle. Proprietary
baseline runs are optional external evidence; neither the product nor the open
acceptance corpus may require a commercial license. Publish only conclusions
supported by that workload, rather than infer platform parity from a favorable
operation or assembly frame rate. These are target benchmark requirements, not
completed comparative measurements.

### Proposed release thresholds

Initial **aspirational gates**, not measured product claims:

| Workload | Proposed acceptance gate |
| --- | --- |
| Regeneration correctness | At least 500 committed topology-changing edit sequences with no silent selector retargeting; ambiguities explicit |
| Shared definitions | Editing one definition rebuilds once across all instances; placement-only changes rebuild no geometry |
| 25,000 instances / 1,000 unique parts | Graphics-only opening deserializes zero exact bodies; explicit memory budget enforced |
| Reference assembly first view | Usable coarse scene within 3 seconds on pinned local NVMe benchmark machine |
| Reference assembly interaction | 1080p opaque inspection p95 frame time ≤16.7 ms on pinned supported discrete GPU |
| Display residency | ≤2 GiB CPU display data and ≤1 GiB GPU data in that benchmark, with visible fallback under pressure |
| 100,000-instance stretch scene | Progressive loading and usable navigation with documented degradation; no unlimited memory growth |
| Exact queries and exports | Results independent of viewer LOD, camera, and asset residency |
| Versioned engineering release | Rebuildable source/lockfiles, validation corpus, support matrix, dependency notices, and open exports |

Before a performance gate can pass, check in dataset generation, exact unique/
repeated-part mix, viewport, tessellation/error settings, build revision, CPU/GPU/
driver/OS, storage/network conditions, and timing instrumentation. Display loading
and exact geometry evaluation get separate timings. Adjust budgets through a
reviewed decision based on evidence, not by relabeling missed targets.

### Recorded first GPU workload sample

At implementation revision `27d7b70`, Chromium 154 with ANGLE SwiftShader on a
16-vCPU QEMU Linux host produced the following single-run observations. Both
workloads use `display_rack.star`, 100 rows × 100 columns, a fresh browser page,
1920 × 1080 viewport at DPR 1, and the same 30-frame camera trace. The actual
canvas is 1580 × 1029 pixels. These are software-rendered observations on a shared
virtual host, not evidence of discrete-GPU frame rate or passed M3 release gates.

| Workload | First view including GPU completion | Frame wall median / p95 | Draw calls / shared meshes | Estimated buffers + picking targets |
| --- | --- | --- | --- | --- |
| 10,000 occurrences, one unique part | 646 ms | 251 / 285 ms | 1 / 1 | 19,830,848 bytes |
| 10,000 occurrences, 100 unique parts | 576 ms | 299 / 583 ms | 100 / 100 | 19,930,640 bytes |

Each run requested only its embedded GPU HTML and zero exact B-rep files. Shared
mesh storage and draw calls scale with unique geometry; the instance buffer is
320,000 bytes in both runs. Allocation estimates exclude driver overhead, CPU
JSON/object memory and the browser's default framebuffer; they are not measured
VRAM residency. Frame samples include `gl.finish()` and therefore GPU completion,
but not presentation latency. First-view timing excludes building the artifacts.
No cold/warm headless-build or repeated-run timing distribution is claimed here.

Raw samples, renderer, environment, parameters and artifact identities are saved
in [repeated-part results](benchmarks/gpu-10000-repeated-swiftshader.json) and
[varied-part results](benchmarks/gpu-10000-varied-swiftshader.json). Reproduce with
`build --display-only --param rows=100 --param columns=100 --param variants=1`
(or `variants=100`), then run `tools/preview-check/gpu-benchmark.cjs` with
`CAD_GPU_URL` set to that served artifact. Its default SwiftShader flags match
these runs; `CAD_GPU_HARDWARE=1` requests the available hardware path and the
report still records the renderer actually obtained.

### Recorded selective-loading sample

At revision `80be037`, the 12-station workcell (60 occurrences, 15 unique meshes)
was inspected in Chromium 154.0.8037.92 with SwiftShader at 1440 × 1000, DPR 1.
The initial camera shows five occurrences and requests four meshes; moving to a
second station reuses three frame meshes and fetches only its new tool. Neither
view requests exact geometry. This is a single local software-rendered run,
not the 25,000-instance or hardware performance gate.

| Observation | Recorded value |
| --- | --- |
| Initial proxy-ready signal | 717 ms from navigation |
| First detailed view, including GPU completion | 737 ms from navigation |
| Second station detailed view, including GPU completion | 117 ms after camera move |
| Resident geometry accounting after first / second view | 7,075 / 8,482 bytes |
| Visible / culled occurrences in each view | 5 / 55 |

The proxy-ready signal is not a measured presentation timestamp. Detailed-view
timings explicitly draw and call `gl.finish()`. Bounds proxies are not simplified
surface LOD. [Raw sample](benchmarks/stream-workcell-swiftshader.json) records
camera, environment, requests, identities and accounting scope. Reproduce with
`node tools/preview-check/stream-benchmark.cjs` after serving the workcell; set
`CAD_BROWSER` to the browser executable and optionally `CAD_STREAM_URL`.

### Recorded analytic part-LOD sample

The 1,000-station spindle line contains 4,000 occurrences and 15 unique part
meshes (12 housing variants plus base, shaft and retainer). A cold browser at
1440×1000 used coarse levels for the overview, then refined the four visible
parts at the origin station to their finest levels. Retainer picking preserved
native face 1; no exact-geometry downloads occurred.

| Observation | Recorded result |
| --- | --- |
| Overview accounted mesh residency | 569,561 bytes versus 3,942,631 bytes for all finest encoded meshes plus geometry buffers |
| Focused station accounted mesh residency | 614,806 bytes; four resident definitions |
| Cold coarse coverage | 1,059 ms |
| Station-detail completion after camera change | 191 ms |
| Unmet geometric detail target / proxies after settling | Zero in both views |

This local Chromium/SwiftShader sample is software-rendered workload evidence,
not a discrete-GPU frame-rate or whole-process memory gate. Timing includes page
loading or refinement fetches and GPU completion, and excludes headless building.
The residency figures exclude browser parse transients, instance buffers and
driver allocations. The [raw record](benchmarks/lod-spindle-line-swiftshader.json)
pins revision, preview and benchmark hashes, parameters, environment and camera
states. Reproduce with the 1,000-station command and
`tools/preview-check/lod-benchmark.cjs` documented in the README. Additional LOD
assets increase publication storage and meshing work; the runtime savings do not
establish bounded exact-body residency during a build.

### Recorded occurrence-BVH acceptance

The occurrence-BVH acceptance fixture contains 25,600 occurrences and eight
shared mesh definitions with multiple rigid rotations. In both embedded and
streamed GPU modes, its focused camera visits 25 hierarchy nodes and tests six
individual occurrences. Visibility is compared against independently transformed
box corners across 76 camera comparisons and clip-boundary contacts; overview and
off-screen queries retain complete candidate sets and original picking IDs.
No exact-body downloads are needed. Run
`npm run test:bvh --prefix tools/preview-check` with `CAD_BROWSER` set to the
supported Chromium binary.

The fast `test:bvh:numeric` regression extracts the production traversal and
compares it with the former linear predicate for 3,000 deterministic camera
cases, including empty/coincident scenes, thin rotated bounds and large coordinate
ranges. These checks establish conservative traversal and pruning for the corpus;
they do not establish browser heap limits, metadata paging or a hardware frame
rate. They exercise the display index, not exact engineering-query completeness.

### Recorded selective-regeneration sample

A local Linux run inspected `workcell_line.star` with 1,000 stations, 25 columns
and 40 tool variants. Three alternating fresh-process samples per mode used the
release CLI without an explicit geometry cache. The full output has 5,000 leaf
occurrences; the separately selected service station has five.

| Mode | Evaluated features / computed unique solids | Median process wall time | Median peak resident memory |
| --- | --- | --- | --- |
| All outputs | 86 / 43 | 80.3 ms | 23,216 KiB |
| Service station | 8 / 4 | 25.9 ms | 21,200 KiB |

This small planar-geometry workload measures compilation, whole-recipe structural
checks, geometry execution, selected assembly materialization and inspect JSON
reporting together. It excludes exports/viewing; OS file caches are uncontrolled.
These shared-host observations are not latency guarantees or evidence of bounded
exact-body memory. [Raw samples](benchmarks/selective-workcell-local.json) include
binary/recipe digests, executor identity, CPU count and per-process `wait4` data.
Reproduce with `python3 scripts/benchmark_selective.py --report /tmp/selective.json`
after building the release CLI. Independent edit tests separately verify zero
new solids for an unselected-tool edit and one for the selected tool.

## Implemented baseline and remaining work

| Area | Implemented now | Target work |
| --- | --- | --- |
| Authoring | Starlark functions/loops/parameters; strict JSON graph; Rust structs; versioned recipe/assembly capability catalog | Locked modules, unit types, source maps, full public agent/SDK contracts |
| Kernel | Orthogonal B-rep/Booleans and bounded parallel-cylinder union/intersection/through-cut; sequential contained through/blind bores and coaxial counterbores in recognized prisms; native simple-profile right prisms with simple polygonal and circular through-holes; translation and scoped exact rigid transforms and bounded convex intersection; single-convex and matched-cap simple-prism validation; analytic circular and simple concave line/arc-profile extrusions and full-turn stepped/tapered revolutions including conical poles with rigid transforms and periodic seams | General curved geometry, generalized intersections/Booleans, cavity/trim handling, richer operations |
| References | Feature-role face selectors, explicit orthogonal/convex-intersection/parallel-cylinder face lineage, cardinality diagnostics, recipe assertions, associative face datums and snapshot witnesses | Instance-path, edge/vertex and predicate selectors, downstream solid-modeling consumers, general-geometry history |
| Assemblies | Schema-2 reusable definitions, nested rigid instances, named local datums and part/rigid-module attachments, explicit paths, shared assets, hierarchical STEP and leaf-part BOM | Joints/constraints, flexible occurrences and configurations |
| Sketches | Original Rust bounded 2D point solver, ten constraint kinds, deterministic independent islands, schema-3 linear/angular residuals and separate tolerances, local rank/DOF/conditioning reports, sketch profiles feeding extrusion and generation-preserving failures | Independent arc variables, circle-to-circle tangency, large connected sparse systems, branch continuation, minimal conflict explanations |
| Builds | Local DAG/cache, selected-output dependency evaluation with global validation, captured source, scope evidence, atomic generations and watch | Bounded exact-body residency, parallel jobs, isolated workers, distributed content store |
| Viewer | HTTP gallery, Canvas with live refresh; opt-in WebGL2 shared-mesh instancing and depth-tested face/occurrence picking; recipe and datum inspection; display-only publication; SHA-256-verified selective mesh streaming, occurrence-BVH frustum culling, bounded geometry cache, native analytic part LOD and proxy fallback | Paged assembly/part spatial indexes, occlusion, cluster LOD/meshlets, GPU generation refresh, sections/overlays |
| Exchange | Planar AP214 and analytic cylinder STEP/STL, shared STEP definitions and named occurrences; independent OCCT/XCAF checks | Analytic import/export, AP242 PMI support, broader exchange fidelity and loss reporting |
| Engineering | Volume/bounds, explicit part metadata/density, nested quantity BOM, summed mass and CSV/JSON reports; native geometric centroid/inertia and density-weighted assembly properties; recipe property assertions with pinned evidence and global validation | Broader sketch/assembly solving, drawings, GD&T, material property libraries, configurations, principal axes and dynamics |

The milestone dependency order and example progression are in [roadmap.md](roadmap.md).
The current working commands are in [README.md](../README.md).

## Supporting-library evaluation

These are supporting-library evaluation areas, not substitutes for our kernel.
`robust` is already pinned and used for adaptive predicates; the other entries
remain candidates. Pin versions, evaluate numerical/performance behavior on our
corpus, and review transitive licenses before adoption. The current dependency
inventory and notices are in [dependency-licenses.md](dependency-licenses.md).

| Candidate | Intended boundary | Upstream license and evidence |
| --- | --- | --- |
| nalgebra | Small matrices, transforms, numerical foundations | Apache-2.0; [repository](https://github.com/dimforge/nalgebra) |
| robust | Adaptive orientation/incircle predicates | Apache-2.0 OR MIT; [repository](https://github.com/georust/robust) |
| faer | Dense/sparse linear algebra for solver experiments | MIT; [sparse solver documentation](https://docs.rs/faer/latest/faer/sparse/linalg/solvers/index.html), [license](https://github.com/sarah-quinones/faer-rs/blob/main/LICENSE) |
| Truck components | Evaluate selected math, tessellation, and interchange implementations/reference designs | Apache-2.0; [repository](https://github.com/ricosjp/truck) |
| wgpu | Native/browser graphics and optional compute | Apache-2.0 OR MIT; [repository](https://github.com/gfx-rs/wgpu) |
| meshopt / meshoptimizer | Display clustering, simplification, compression, reordering | Wrapper Apache-2.0 OR MIT; C++ implementation MIT; [wrapper](https://github.com/Traverse-Research/meshopt-rs), [upstream](https://github.com/zeux/meshoptimizer) |

`meshopt` is an FFI wrapper, not an all-Rust implementation. Permissive supporting
FFI can be evaluated without outsourcing the original modeling kernel. Predicate
and linear-algebra libraries do not provide a complete reliable sketch solver,
trimmed-surface intersector, or persistent naming algorithm.

Current wgpu documentation exposes experimental native-only mesh shaders and
backend-limited double-precision support. Neither is the portable browser
baseline. These feature details can change and must be checked against the
selected release and device capabilities. [Mesh-shader feature](https://docs.rs/wgpu/latest/wgpu/struct.Features.html#associatedconstant.EXPERIMENTAL_MESH_SHADER),
[double-precision feature](https://docs.rs/wgpu/latest/wgpu/struct.Features.html#associatedconstant.SHADER_F64).

## Evidence and interpretation of the supplied notes

The supplied notes are architectural input, not benchmark evidence about named
competitors. We adopt lazy loading, persistent design references, separate
assembly solving, and streamed display assets; broad product superiority claims
and unmeasured timing/percentage claims are not requirements.

Autodesk documents a graphics-cache Express mode and loading component data on
demand. That supports the representation separation; it does not establish
universal assembly-size or frame-rate guarantees. [Autodesk Express Mode](https://help.autodesk.com/cloudhelp/2026/ENU/Inventor-Help/files/GUID-34C9AE4C-EDA7-4E46-AD51-3383A67FF1B9.htm).

Epic documents hierarchical triangle clusters, view-dependent detail and demand
streaming, while explicitly retaining practical content/hardware limits. These
are useful display-design principles. We are not adopting Unreal/Nanite code or
claiming that mesh virtualization provides an exact CAD kernel. [Epic Nanite overview](https://dev.epicgames.com/documentation/en-us/unreal-engine/nanite-virtualized-geometry-in-unreal-engine).

Meshoptimizer supplies clustering and simplification building blocks, and its
upstream documents a companion clustered-LOD library. Evaluate these behind the
display-worker interface; adopting a clusterizer alone does not implement our
streaming scheduler, CAD semantic mapping or conservative error contract.
[Meshoptimizer documentation](https://meshoptimizer.org/).

OCCT itself provides shape analysis, repair, tolerance handling, and status
reporting. A blanket assumption that it lacks healing is incorrect; robustness
comparisons require a reproducible corpus. [OCCT Shape Healing](https://occt3d.com/dev/doc/overview/html/occt_user_guides__shape_healing.html).
