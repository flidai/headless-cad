# Product roadmap and release gates

Goal: an Apache-2.0, original-Rust, headless AI-native CAD system competitive with
commercial mechanical-design platforms. See [architecture.md](architecture.md)
for binding architectural direction. This is a capability roadmap, not a promise
of completion dates or an assertion that the present implementation has parity.

Every milestone requires a documented capability envelope, independent expected
results, deterministic failure behavior, source-visible examples, and measured
regression performance. A green test count alone does not close a milestone.

| Milestone | Deliverable and example progression | Acceptance evidence | Status |
| --- | --- | --- | --- |
| M0: executable foundation | Recipes, orthogonal B-rep, validated builds, STEP/STL, HTTP inspection, source panel, watch. Plate → bracket → tray → fixture → vented enclosure | Analytic volume/bounds/topology checks, parameter edits, independent STEP imports, cache/publication/browser tests | Working bounded baseline; maintain continuously |
| M1: design identity and assembly model | Versioned part definitions, instances, rigid transforms, datums, semantic references, lineage. Fixtures become true component assemblies; repeated fasteners share one definition | One definition edit computes once; placement changes compute no new geometry; 500 committed reference/edit sequences have no silent retargeting | In progress: nested instances, shared assets/hierarchical STEP, scoped face-role queries and orthogonal lineage implemented; broader selectors, datums and downstream modeling consumers remain |
| M2: general planar and analytic geometry | Planar profiles with holes, general rigid transforms, circles/arcs/cylinders, revolution, correct trim/seam handling and scoped Booleans. Angled fixture → drilled mounting bracket → shaft/collar → supported bearing housing | Independent formula/integral checks, thin/tangent/coincident adversarial cases, explicit unsupported inputs, analytic STEP checks | In progress: strict convex CCW polygon extrusion (3–256 vertices), polygon caps and single-convex-solid planar validation implemented; holes, curves, general transforms/Booleans and trim/seam handling remain |
| M3: scalable inspection | Meshing interface, shared display assets, GPU instancing, metadata-first loading, bounded residency, LOD/cluster refinement. Parametric rack with thousands of repeated components | 10k-instance initial benchmark then 25k gate; zero exact loads for overview; fixed camera traces and stated hardware/error budgets | Planned; independent of curved-kernel completion |
| M4: design constraints | 2D sketches, 3D rigid-joint graphs, residual/DOF/conflict reporting and branch stability. Four-bar mechanism → slider fixture → constrained gearbox layout | Over/underconstrained cases, singular positions, edit continuation, joint residual bounds, collision distinct from constraint solving | Planned |
| M5: demanding solid modeling | Sweeps/lofts, general curved intersections, chamfers/fillets, offsets/shelling, controlled repair. Filleted bearing housing → ribbed enclosure → swept manifold | Trimmed-surface validity, repair-deviation reports, failure minimization, broad edit corpus, independent exchange/query results | Planned; subdivide by operation |
| M6: manufacturing deliverables | Associated drawings/sections, dimensions/datums/GD&T intent, materials, mass/inertia, BOM, analytic/assembly STEP and PMI support matrix. Releasable multi-part fixture with drawing pack | Stable annotation edits, material/quantity rollups, no LOD-dependent measurements, export fidelity/loss reports | In progress: part metadata, nested quantity BOM and summed density-based mass implemented; drawings, PMI, inertia and configuration policies remain |
| M7: team-scale engineering | Pinned releases, semantic review, isolated job service, artifact distribution, cancellation/recovery, revision/BOM adapters. Multi-contributor machine module | Reproducible revisions, isolation/quotas, concurrent publication, slow-network/offline recovery, audited artifact identity | Planned |

Completing M1 and extending the display-asset interface are the immediate
architectural priorities. Nested assemblies now share definitions and display
meshes; the viewer still embeds its scene and expands occurrences for Canvas
rendering. Selective asset loading and GPU instancing are the next steps toward
scale. Numerical kernel development continues in parallel through M2; GPU display work does not block headless geometry or substitute for it.

M1 instance transforms describe placement and display reuse. Baking rotated exact
geometry or applying modeling operations across transformed instances remains
subject to M2 geometric support; placing an instance does not imply those
operations are already supported.

## Next concrete implementation increments

1. Extend implemented schema-2 nested assemblies with named datums and explicit
   configuration identity. Schema 1 remains supported; keep migration, nested
   transform composition, and repeated-definition reuse tests.
2. Extend implemented face-role selector results (`resolved`, `deleted`,
   `ambiguous`, `unsupported`) and orthogonal face lineage to richer predicates,
   datums and downstream modeling consumers. Recipe `assert_faces` now enforces
   these references before publication and preserves reports for review.
   Expand the generated edit corpus beyond orthogonal cuts and translations
   before claiming general persistent naming reliability.
3. Extend the implemented metadata-only assembly manifest and shared local-mesh
   display format with versioned error contracts, GPU instancing, asset fetching,
   and residency budgets. Preserve occurrence/face identity through refinement.
4. Expand the converted fixture/enclosure and repeated mounting-array examples
   into measured benchmarks varying unique-part count as well as repetition.
5. Extend the implemented convex polygon extrusion with a general geometry/trim
   contract and profiles with holes before curved Boolean claims. Preserve the
   current explicit limits: strict CCW 3–256-vertex profiles, one general convex
   solid per kernel snapshot, and orthogonal-only Booleans. Polygon `wall_i`
   roles follow input edge order; translation has lineage, general polygon
   Booleans do not. Retain triangle/trapezoid/pentagon analytic and STEP oracles,
   plus `angled_fixture.star` and the browser `test:polygon` silhouette/picking
   regression as curved primitives and trim handling are introduced.

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
