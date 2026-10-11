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

| Milestone | Deliverable and example progression | Acceptance evidence | Status |
| --- | --- | --- | --- |
| M0: executable foundation | Recipes, orthogonal B-rep, validated builds, STEP/STL, HTTP inspection, source panel, watch. Plate → bracket → tray → fixture → vented enclosure | Analytic volume/bounds/topology checks, parameter edits, independent STEP imports, cache/publication/browser tests | Working bounded baseline; maintain continuously |
| M1: design identity and assembly model | Versioned part definitions, instances, rigid transforms, datums, semantic references, lineage. Fixtures become true component assemblies; repeated fasteners share one definition | One definition edit computes once; placement changes compute no new geometry; 500 committed reference/edit sequences have no silent retargeting | In progress: nested instances, named part datums/sibling attachments, shared assets/hierarchical STEP, scoped face-role queries and orthogonal lineage implemented; associative planar face datums implemented; broader selectors and downstream solid-modeling consumers remain |
| M2: general planar and analytic geometry | Planar profiles with holes, general rigid transforms, circles/arcs/cylinders, revolution, correct trim/seam handling and scoped Booleans. Angled fixture → drilled mounting bracket → shaft/collar → supported bearing housing | Independent formula/integral checks, thin/tangent/coincident adversarial cases, explicit unsupported inputs, analytic STEP checks | In progress: strict convex CCW polygon extrusion (3–256 vertices), polygon caps and single-convex-solid planar validation implemented; scoped exact rigid transforms and bounded convex intersection with face lineage implemented; holes, curves, general nonconvex transforms/Booleans and trim/seam handling remain |
| M3: scalable inspection | Meshing interface, shared display assets, GPU instancing, metadata-first loading, bounded residency, LOD/cluster refinement. Parametric rack with thousands of repeated components | 10k-instance initial benchmark then 25k gate; zero exact loads for overview; fixed camera traces and stated hardware/error budgets | In progress: opt-in WebGL2 shared-mesh instancing, depth-tested picking, display-only publication and repeat/unique workload harness; selective verified mesh streaming, frustum culling and bounded cache implemented; LOD, spatial hierarchy and hardware gates remain |
| M4: design constraints | 2D sketches, 3D rigid-joint graphs, residual/DOF/conflict reporting and branch stability. Four-bar mechanism → slider fixture → constrained gearbox layout | Over/underconstrained cases, singular positions, edit continuation, joint residual bounds, collision distinct from constraint solving | Planned |
| M5: demanding solid modeling | Sweeps/lofts, general curved intersections, chamfers/fillets, offsets/shelling, controlled repair. Filleted bearing housing → ribbed enclosure → swept manifold | Trimmed-surface validity, repair-deviation reports, failure minimization, broad edit corpus, independent exchange/query results | Planned; subdivide by operation |
| M6: manufacturing deliverables | Associated drawings/sections, dimensions/datums/GD&T intent, materials, mass/inertia, BOM, analytic/assembly STEP and PMI support matrix. Releasable multi-part fixture with drawing pack | Stable annotation edits, material/quantity rollups, no LOD-dependent measurements, export fidelity/loss reports | In progress: part metadata, nested quantity BOM, native centroid/central inertia and density-weighted assembly queries implemented; drawings, PMI, principal axes and configuration policies remain |
| M7: team-scale engineering | Pinned releases, semantic review, isolated job service, artifact distribution, cancellation/recovery, revision/BOM adapters. Multi-contributor machine module | Reproducible revisions, isolation/quotas, concurrent publication, slow-network/offline recovery, audited artifact identity | Planned |

Completing M1 and extending the display-asset interface are the immediate
architectural priorities. Nested assemblies now share definitions and display
meshes. The opt-in WebGL2 viewer uses GPU instancing; Canvas remains a portable
fallback. The GPU client now also supports metadata-first selective mesh loading,
frustum culling and bounded geometry residency with visible proxies. Spatial
hierarchies, bounded-error LOD and hardware performance gates remain. Numerical kernel development continues in parallel through M2; GPU display work does not block headless geometry or substitute for it.

M1 instance transforms describe placement and display reuse. The M2 `transform`
operation now bakes rigid rotations and translations into exact geometry within
its single-convex-solid or exact axis-permutation domain. It does not enable
general union/cut across rotated solids or modeling across assembly instances. `tilted_jig.star`
adds two transformed tools with cap/wall lineage assertions; manufacturing files
contain their transformed native coordinates.

## Next concrete implementation increments

1. Extend implemented schema-2 nested assemblies and named part datums with
   cross-level attachment semantics and explicit
   configuration identity. Sibling rigid attachments now evaluate a bounded DAG
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
   contract and profiles with holes before curved Boolean claims. Preserve the
   current explicit limits: strict CCW 3–256-vertex profiles, one general convex
   solid per kernel snapshot, orthogonal-only union/cut and at most 48 input
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
