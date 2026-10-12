# Product roadmap and release gates

Goal: an Apache-2.0, original-Rust, headless AI-native CAD system competitive with
commercial mechanical-design platforms. See [architecture.md](architecture.md)
for binding architectural direction. This is a capability roadmap, not a promise
of completion dates or an assertion that the present implementation has parity.

Every milestone requires a documented capability envelope, independent expected
results, deterministic failure behavior, source-visible examples, and measured
regression performance. A green test count alone does not close a milestone. The reference/edit corpus
must record expected intent across feature insertion/reordering, split/merge
histories, patterned-instance replacement, suppression and imported geometry;
repeatable selection alone is insufficient. Large-assembly gates include a
one-component edit with bounded rebuild count and peak exact-body memory, not
only opening the viewer. A manufacturing gate includes a complete revised
release pack with associated views, dimensions, datum references, BOM and an
explicit exchange-loss report.

The first complete engineering workflow is a configurable bearing-supported
drive module, developed from the shaft and cartridge recipes through a machined
housing, fits, constrained motion and a revised manufacturing release pack.
See the architecture's [reference workflow](architecture.md#reference-engineering-workflow).
Track planned, experimental and validated capabilities with their supported
domains and reproducible evidence. Pair this modeling workflow with a separate
large workcell benchmark; neither substitutes for the other's release gates.

Use the architecture's [commercial workflow benchmark protocol](architecture.md#commercial-workflow-benchmark-protocol)
to judge progress toward Autodesk-class workflows. Track supported task outcomes,
revision reliability and manufacturing fidelity before comparing speed. Keep
commercial baseline evidence optional so the Apache-licensed product and its
open acceptance corpus remain independently usable.

| Milestone | Deliverable and example progression | Acceptance evidence | Status |
| --- | --- | --- | --- |
| M0: executable foundation | Recipes, orthogonal B-rep, validated builds, STEP/STL, HTTP inspection, source panel, watch. Plate → bracket → tray → fixture → vented enclosure | Analytic volume/bounds/topology checks, parameter edits, independent STEP imports, cache/publication/browser tests | Working bounded baseline; maintain continuously |
| M1: design identity and assembly model | Versioned part definitions, instances, rigid transforms, datums, semantic references, lineage. Fixtures become true component assemblies; repeated fasteners share one definition | One definition edit computes once; placement changes compute no new geometry; 500 committed reference/edit sequences have no silent retargeting | In progress: nested instances, named part datums with part/rigid-module attachments, shared assets/hierarchical STEP, scoped face-role queries and orthogonal lineage implemented; associative planar face datums implemented; broader selectors and downstream solid-modeling consumers remain |
| M2: general planar and analytic geometry | Planar profiles with holes, general rigid transforms, circles/arcs/cylinders, revolution, correct trim/seam handling and scoped Booleans. Angled fixture → drilled mounting bracket → shaft/collar → supported bearing housing | Independent formula/integral checks, thin/tangent/coincident adversarial cases, explicit unsupported inputs, analytic STEP checks | In progress: simple polygon outer profiles with simple nonconvex holes, exact rigid transforms of supported prisms, and bounded convex planar intersection with lineage implemented; analytic disk/annular extrusions, mixed polygon/circular through-hole prisms and their periodic seams implemented; full-turn piecewise-linear-profile revolution including regular conical frusta and conical poles implemented; convex line/arc path extrusion with circular holes implemented; concave arc paths, arcs in revolved meridians, partial revolution, general Booleans and generalized trim/seam handling remain |
| M3: scalable inspection | Meshing interface, shared display assets, GPU instancing, metadata-first loading, bounded residency, LOD/cluster refinement. Parametric rack with thousands of repeated components | 10k-instance initial benchmark then 25k gate; zero exact loads for overview; fixed camera traces and stated hardware/error budgets | In progress: opt-in WebGL2 shared-mesh instancing, depth-tested picking, display-only publication and repeat/unique workload harness; selective verified mesh streaming, occurrence-BVH frustum culling and bounded cache and analytic whole-part LOD implemented; metadata paging, cluster LOD, part spatial indexes and hardware gates remain |
| M4: design constraints | 2D sketches, 3D rigid-joint graphs, residual/DOF/conflict reporting and branch stability. Dimensioned slope fixture → four-bar mechanism → constrained gearbox layout | Over/underconstrained cases, singular positions, edit continuation, joint residual bounds, collision distinct from constraint solving | In progress: bounded original Rust point sketch solver, fixed/coincident/horizontal/vertical/distance constraints, polygon extrusion, local rank/residual/conditioning reports and atomic failure handling; curves, sparse islands, branch continuation, conflict minimization and assembly solving remain |
| M5: demanding solid modeling | Sweeps/lofts, general curved intersections, chamfers/fillets, offsets/shelling, controlled repair. Filleted bearing housing → ribbed enclosure → swept manifold | Trimmed-surface validity, repair-deviation reports, failure minimization, broad edit corpus, independent exchange/query results | Planned; subdivide by operation |
| M6: manufacturing deliverables | Associated drawings/sections, dimensions/datums/GD&T intent, materials, mass/inertia, BOM, analytic/assembly STEP and PMI support matrix. Releasable multi-part fixture with drawing pack | Stable annotation edits, material/quantity rollups, no LOD-dependent measurements, export fidelity/loss reports | In progress: part metadata, nested quantity BOM, native centroid/central inertia and density-weighted assembly queries and global recipe property assertions implemented; drawings, PMI, principal axes and configuration policies remain |
| M7: team-scale engineering | Pinned releases, semantic review, isolated job service, artifact distribution, cancellation/recovery, revision/BOM adapters. Multi-contributor machine module | Reproducible revisions, isolation/quotas, concurrent publication, slow-network/offline recovery, audited artifact identity | Planned |

Completing M1 and extending the display-asset interface are the immediate
architectural priorities. Nested assemblies now share definitions and display
meshes. The opt-in WebGL2 viewer uses GPU instancing; Canvas remains a portable
fallback. The GPU client now also supports metadata-first selective mesh loading,
occurrence-BVH frustum culling and bounded geometry residency with visible proxies.
Analytic parts now refine through up to three native tessellation levels with
explicit unmet-detail reporting. Paged metadata, part spatial hierarchies, cluster
LOD and hardware performance gates remain. Numerical kernel development continues in parallel through M2; GPU display work does not block headless geometry or substitute for it.

M1 instance transforms describe placement and display reuse. The M2 `transform`
operation now bakes rigid rotations and translations into exact geometry for
supported convex solids, simple-profile and mixed analytic prisms, cylinders,
tubes, and full-turn revolved solids; orthogonal multi-solid geometry additionally
supports exact axis permutations. It does not enable
general union/cut across rotated solids or modeling across assembly instances. `tilted_jig.star`
adds two transformed tools with cap/wall lineage assertions; manufacturing files
contain their transformed native coordinates.

## Next concrete implementation increments

The first M3a increment now streams whole-part analytic LOD, preserving current
surfaces while finer assets load and exposing unmet detail under resource or
precision limits. `spindle_line.star` combines tapered housing variants with
shared drilled bases, shafts and rings. Continue toward metadata paging and
cluster refinement; a working part-LOD viewer does not close the hardware or
full error-certification gates.

For scalable inspection, follow the architecture's
[staged display gates](architecture.md#target-display-hierarchy-contract):
M3a bounded-error part LOD, M3b paged assembly metadata, M3c cluster streaming,
then M3d measured visibility acceleration. Each stage includes selection/source
identity, degraded-detail behavior and bounded resources. WebGPU or meshlets
alone do not close M3. Separate within-part cluster refinement from assembly
HLOD aggregation: aggregate selection must resolve actual occurrence ownership,
and hide/isolate/section changes must preserve correct coverage. Track display
preprocessing time, temporary disk, hierarchy storage amplification and
single-definition invalidation alongside viewer memory and frame times. These
are planned acceptance obligations, not capabilities of today's whole-part LOD.
The display error ledger must include native endpoint/support discrepancies and
coordinate/precision effects. Cluster refinement must track demand per occurrence
and view while deduplicating shared pages; cancellation cannot remove another
occurrence's required coverage. Gate both contracts with adversarial fixtures
before declaring M3c complete.
In parallel, measure exact-body residency during a
one-part edit and full release generation so viewer progress does not conceal
headless build costs. Keep M2 geometry reliability, M4 constraints and M6
manufacturing acceptance independent of display performance.

Engineering property gates now enforce native volume, mass, centroid and central
inertia bounds before publication, including selected-output builds.
`mounted_drive_module.star` carries fixed mass and balance requirements; its
independent tests reject overbudget edits while preserving geometry cache reuse.
Extend these obligations toward dimensions, clearance and manufacturing intent
as those query contracts become available.

Apply the architecture's request/representation contracts to each increment:
opening and picking are read-only display/source requests; modeling, solving,
validated measurements and releases are explicit headless jobs. Preserve this
separation in acceptance tests as the viewer becomes more capable.

The first analytic example, `shaft_stack.star`, now combines true circular
extrusions, periodic seams, independent mass-property formulas, cap datums and
analytic STEP validation. Its display/STL tessellation has a declared chord-error
policy; native cylinders remain analytic. `bearing_cartridge.star` now adds true hollow sleeves and shared retainer rings,
with bore picking, mixed-material mass/inertia and associative placement checks.
Expand toward a bearing housing as curved operations become available.
`drilled_bearing_mount.star` adds a polygon plate with mixed polygonal and true
circular through-holes, a centroid-associated sleeve and shaft, and four nested
bolt assemblies. Circular walls retain native face roles and analytic STEP;
this extends profile extrusion without claiming general curved Boolean support.
`stepped_spindle.star` adds connected stepped shafts, a counterbored housing and
a blind-bored cap through full-turn orthogonal-profile revolution. Native plane/
cylinder topology, shared circular boundaries, analytic moments and independent
STEP checks cover this operation. `tapered_nozzle.star` extends this to chamfered/tapered walls and countersinks.
Conical poles now support solid cones, double-tip solids and pointed blind bores,
with true pole vertices, analytic moments and independent STEP validation.
`centering_fixture.star` places two shared center pins in opposite workpiece
bores using datum attachments, supported by drilled holders and a base. Its
length/clearance/depth parameter corpus checks bounds, native material properties,
balance and seating frames. Depth edits rebuild two definitions while preserving
total material volume; release assertions remain active for selected-body builds.
This does not certify contact, clearance or interference. Partial turns, arcs in revolved
meridians, isolated axis contacts, pinched shells and general curved Booleans remain
unsupported.
`rounded_clevis.star` now exercises native trimmed circular edges in convex
line/arc-profile extrusions: a rounded base, two shared arched cheeks, a capsule
link and pin form five occurrences from four definitions. Kernel and independent
exchange acceptance passes; analytic browser rays check cap, arc-wall and bore
picking through all three display clients, including oblique occurrences.
Independent component formulas and a 27-case parameter corpus check link volume,
centroid/full inertia, assembly material properties, bounds and datum placement.
Length/radius edits rebuild one definition each, gap edits two. Preserve explicit
CCW convex-profile, half-turn-arc, strict circular-hole separation and numerical
limits; concave arc profiles, arc meridians and general curved Booleans remain
future extensions. Native engineering assertions remain build gates even for
selected-body publication.

Subsequent examples should combine new operations with
reference-preserving parameter edits and engineering deliverables, not only add
more copies of existing parts.

1. Extend implemented schema-2 nested assemblies and named part datums with
   explicit configuration identity, suppression policies and reference-preserving
   edits. Whole rigid subassemblies now attach through a descendant source datum;
   mixed part/module cycles fail before modeling and shared internal placements
   remain immutable. `mounted_drive_module.star` docks two reusable modules to
   base datums and attaches an external sensor through an already placed module.
   Descendant target paths now attach a local part to a named datum inside a
   reused child module, without upward dependencies or geometry duplication.
   `instrumented_cartridge.star` demonstrates this with a sensor mounted to a
   nested housing. Sibling rigid attachments evaluate a bounded DAG
   using target placement × target datum × local offset × inverse own datum;
   they are not a nonlinear mate solver. Preserve the source intent/resolved
   placement distinction, schema migration, noncommuting transform tests and
   repeated-definition reuse. `datum_gantry.star` progresses from attached posts
   and a rotated tool frame to a repeated inspection line; STEP preserves the
   resulting placements but not the datum/attachment semantics.
2. Extend implemented face-role selector results (`resolved`, `deleted`,
   `ambiguous`, `unsupported`) and orthogonal face lineage to richer predicates,
   richer downstream modeling consumers. Associative planar face datums now drive
   existing attachments with an area-centroid frame and explicit orientation hint. Recipe `assert_faces` now enforces
   these references before publication and preserves reports for review.
   Expand the generated edit corpus beyond orthogonal cuts and translations
   before claiming general persistent naming reliability.
3. Extend the implemented metadata-only assembly manifest and shared local-mesh
   display format with versioned geometric error contracts, spatial indexing and
   bounded-error LOD. Verified asset fetching, frustum culling, request limits,
   stale-response rejection and a bounded geometry cache are implemented. Its
   workload limits are not interactive performance guarantees. Preserve
   occurrence/face identity through refinement. Follow the target display hierarchy
   contract: versioned page descriptors, conservative error bounds, complete
   replacement dependencies, separate assembly/part spatial indexes and visible
   degraded-detail status when a budget prevents the requested refinement.
4. Expand the converted fixture/enclosure and repeated mounting-array examples
   into measured benchmarks varying unique-part count as well as repetition.
   `display_rack.star` now varies rows, columns and unique geometry independently;
   run large cases with `--display-only` to separate inspection from exchange work.
   `workcell_line.star` combines datum attachments and convex tools in 12 stations
   (60 occurrences, 15 part definitions). A station-focused camera needs only
   three shared frame meshes and its own tool; panning to a different station
   should reuse the frame meshes and fetch one new tool.
5. Extend the implemented convex polygon extrusion with a general geometry/trim
   contract and general planar Booleans before curved Boolean claims.
   Simple-ring through-hole extrusion now preserves true inner cap loops,
   signed material moments, semantic wall roles and hole-aware STEP/display output. Preserve the
   current explicit limits: simple CCW profiles with at most 256 total vertices
   and 32 disjoint simple holes, one general convex solid or certified right prism per
   kernel snapshot, orthogonal-only union/cut and at most 48 input
   planes for general convex intersection. `clipped_tools.star` adds oblique
   shoulders and an octagonal crop, with references inherited from both operands. Polygon `wall_i`
   roles follow input edge order; translation/rigid transforms and supported convex intersection have lineage;
   general polygon union/cut remain unsupported. Retain triangle/trapezoid/pentagon analytic and STEP oracles,
   plus `angled_fixture.star` and the browser `test:polygon` silhouette/picking
   regression as curved primitives and trim handling are introduced.

Selected-output execution now evaluates a requested dependency closure while
retaining complete structural checks and all global semantic obligations.
`workcell_line.star --output service_station` builds four unique solids; an edit
to an unselected tool computes none, and an edit to the selected tool computes
one. A 1,000-station/40-tool test verifies the same four-solid closure. This is
rebuild-count evidence, not a peak-memory or latency gate: bounded exact-body
residency, parallel scheduling and isolated workers remain to implement.

The through-hole increment uses `perforated_mount.star`: an octagonal plate
with rectangular and triangular openings, four supports, and a locator attached
to the remaining top face's centroid. Varying a hole width tests associative
placement and independently integrated mass properties. Browser rays through
both holes must select the base below; STEP must preserve inner face bounds.
`fork_mount.star` extends this to a U-shaped outer profile with openings in both
arms, explicit offset datum placement, and independent box-subtraction moments
before/after native rotation. Boundary validation rejects holes crossing the
open notch even when their vertices are in the arms. The prototype now supports
simple nonconvex outer and inner rings alongside circular through-holes. General
hole Booleans remain unsupported.

`serpentine_manifold.star` adds a simple nonconvex channel hole alongside circular
fastener bores, a floor and ported cover, two fittings and four reused bolts.
Nine occurrences share five definitions; `open_channel` provides an exposed
inspection configuration. Width/depth edits each rebuild two definitions while
native volume and mass assertions gate publication. Independent component sums
check volume and full inertia; kernel acceptance includes disjoint holes with
overlapping hulls, concave-bay circles, transformed recognition and rejection of
nesting, crossing, tangency and uncertain separation. Keep the 256-vertex and
32-combined-hole limits and certified display triangulation. This extends
profile extrusion without introducing general hole Booleans or pressure/seal
certification.

The M6 mass-properties increment uses `balance_arm.star`: a rotated beam,
mixed-density payload/counterweight, and rigid attachment chain. Position edits
change the material center and inertia while reusing part geometry. Independent
box-moment expectations distinguish volume centroid from center of mass and
check the full tensor at three counterweight positions. Missing densities and
numerical limitations remain explicit; this is not a dynamics or joint solver.

The M1 associative-datum increment uses `surface_mount.star`: editing the ramp's
rise changes both the attached plate's position and angle through a semantic
face reference. Independent plane equations and STEP placement checks cover the
successful case; split/deleted faces and parallel orientation hints must fail
without replacing the last valid artifacts. This is a scoped reference consumer,
not a general sketch-on-face tool or a nonlinear assembly solver.

The first M4 increment now solves the named-point sketch in
`constrained_slope_fixture.star` from three edge lengths. An inclined face datum
positions its deck and rails after every successful solve. Independent
circle-intersection formulas cover 27 parameter combinations; tests distinguish
underconstraint, inconsistent local residuals and numerical limitations, and
verify failed builds retain the prior published generation. Fully constrained
means converged with supported local Jacobian rank and conditioning; it does not
prove global uniqueness or branch continuity. Keep structural validation global,
solve only demanded sketches, and retain solver evidence even on cache hits.
Advance toward constrained curves, angles/tangency and explicit continuation
before claiming general sketch coverage; rigid assembly joints are a separate
solver milestone.

## Example policy

Every new modeling tool must have a small isolating recipe and at least one
ambitious composition that uses it with existing tools. Keep the original small
fixtures so failures remain diagnosable. Expose useful parameters, retain exact
source in the viewer, and record independent expected measurements. Include
expected failures and capability limits alongside successful demonstrations.

A visually elaborate tessellation is not evidence of a more capable exact
kernel. Distinguish display benchmarks, topology/geometry benchmarks, constraint
benchmarks, and engineering release examples. Assembly performance reports state
both instance count and unique-definition count.

## Meaning of completion

The current orthogonal product and convex polygon increment close neither the commercial-platform
goal nor the kernel roadmap. A credible commercial-competition release requires
a published supported-workflow matrix, measured assembly and regeneration
behavior, reliable manufacturing exchange/deliverables, reproducible source-driven
workflows, and completed gates for its declared scope. Unsupported cases remain
explicit. The overall project goal stays active as these milestones are built.
