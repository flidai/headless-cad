# Product roadmap and release gates

Goal: an Apache-2.0, original-Rust, headless AI-native CAD system competitive with
commercial mechanical-design platforms. See [architecture.md](architecture.md)
for binding architectural direction. This is a capability roadmap, not a promise
of completion dates or an assertion that the present implementation has parity.

The competitive reference is Autodesk-class engineering workflows. The supplied
commercial-CAD and virtualized-geometry briefs feed five parallel release tracks:
robust modeling and persistent references (M1/M2/M5), bounded assembly inspection
(M3), mechanical constraints (M4), manufacturing deliverables (M6), and reproducible
team automation (M7). None is closed by completing a renderer or accumulating
passing examples. First-party delivery remains Apache-2.0, with an original Rust
kernel and source-authoritative modeling throughout.

Implementation reviews must use the architecture's
[commercial engineering and virtualized inspection decision](architecture.md#architecture-decision-commercial-engineering-and-virtualized-inspection).
Require explicit API/artifact ownership for exact-body admission, metadata pages,
cluster replacement and revision-bound manufacturing packs. Prioritize metadata
paging before cluster-streaming acceleration; pursue kernel, constraint and
manufacturing coverage in parallel. Keep all stages open until their own evidence
passes, including unique-part workloads and failure recovery.
Normal-operation display acceptance must reach the declared detail target within
its predeclared latency budget. Visible proxies and unmet-detail reports establish
safe overload behavior only; they cannot satisfy the normal-operation gate.
The complete first release requires both the drive-module engineering workflow
and the separate assembly-scale workload.
Track completion in the [commercial release readiness ledger](architecture.md#commercial-release-readiness);
all five product gates remain open, independently of individual increment acceptance.

M3's large-assembly gate also includes the architecture's
[exact-body admission contract](architecture.md#exact-body-admission-and-oversized-operations):
test one oversized unique-body operation as well as repeated-part assemblies,
and measure concurrent inspection, revision and export. Bounded display memory
does not establish bounded modeling memory. Resource failures must preserve the
previous complete generation without reducing engineering fidelity. M7 adapters
must reconcile idempotent release deliveries and expose partial synchronization.

The metadata-page producer now publishes a small source-bound index and immutable
spatial branch/leaf pages with original occurrence identity and independent mesh
references. Atomic publication and integrity checks cover the page set. This is
the M3b artifact foundation. A separate experimental paged inspector now adds
verified page scheduling, bounded record counts, eviction and subtree proxies.
Legacy viewers continue to load the complete occurrence catalog. The new path
now also integrates adaptive whole-part mesh detail, retained coarse coverage and
explicit unmet-error reporting. Declared hardware/detail latency gates and cluster
refinement remain open; resource accounting is distinct from measured total browser heap.

Use the [feature-brief acceptance scenarios](architecture.md#feature-brief-acceptance-scenarios)
to turn these tracks into release checks. Increase unique geometry complexity,
feature-history depth, occurrence count, constraint coupling and annotation count
independently. Record failures and unsupported inputs as well as successful runs;
neither instance count nor test count alone measures commercial readiness.

The [first competitive release contract](architecture.md#first-competitive-release-contract)
requires five distinct scenarios: large-machine inspection, component revision,
mechanical-intent validation, manufacturing release, and reproducible human/agent
changes. Retain evidence for each against the same declared release scope.
The native section-query increment implements a bounded M6 foundation: native
line/arc curves, area/centroid, occurrence identity and source-bound SVG. These
queries do not close associative drawings, dimensions, fits or GD&T.

Source-authored `section_view` declarations now regenerate JSON/SVG inspection
views inside atomic generations. Their optional area bounds are global build
obligations even for selected outputs and cache hits. Four bearing-cover views
follow source parameters and retain body/occurrence provenance. Continue M6 toward
semantic annotation references, projected/hidden-line views and dimensions; these
parameter-associated planes alone do not satisfy the drawing release gate.
Datum-anchored section planes now follow exact nested leaf occurrences and named
part frames, including semantic planar face datums. The repeated housing fixture
checks 27 native configurations and three independently imported STEP poses;
these associative sections still leave projected views, dimensions and GD&T open.

Projected X/Y datum-to-datum dimensions now attach to section views, carry
semantic endpoint witnesses and enforce global design bounds. The repeated
housing example dimensions station pitch through assembly rotations. Continue
toward geometric edge/curve dimensions, projected views, annotation layout and
manufacturing semantics; this increment does not close the drawing release gate.

Semantic radius/diameter annotations now resolve generated faces through lineage
and measure their complete native section circles. The conical-seat examples
exercise section-depth changes and repeated rotated occurrences with independent
STEP circle checks. Partial or ambiguous intersections reject rather than becoming
mesh measurements; broader drawing semantics remain a release obligation.

Parallel circular-extrusion clearance advances the bearing workflow to real
lateral offsets: eccentric solid/hollow spindles, exterior rings and nested
annuli. `parallel_clearance.star` includes overlapping annular footprints whose
axially separated witnesses lie away from the centerline. Independent exact
radial/axial formulas, STEP boundary witnesses, reversed/common poses and
publication-preserving failures are required evidence. Offset noncircular
meridians, nonparallel annuli and continuous motion remain open.

Coaxial native clearance now advances the bearing workflow beyond convex shapes:
actual hollow bores, stepped shafts and concave meridians retain their material
regions. `coaxial_clearance.star` combines radial bearing and axial spacer checks
with explicit query-method reports. Independent profile distances, STEP boundary
witnesses, pose/reversal cases, containment rejection and preserved generations
are required evidence. Noncoaxial nonconvex geometry, manufactured envelopes and
continuous-motion clearance remain open.

Convex linear-meridian revolutions extend native clearance toward tapered drive
components: cones, frustums and convex crowned profiles. The shared kernel admission
contract must reject hollow and waisted profiles before placement/query work.
`revolved_clearance.star` requires independent STEP distance/witness checks,
analytic radial/axial cases, pose invariance and preserved failed generations.
General trimmed/nonconvex clearance and continuous motion remain open.

Analytic clearance extends the native query to finite solid cylinders in arbitrary
rigid poses and mixed cylinder/convex-planar pairs. `cylinder_clearance.star` adds
skew rollers and a tilted guard with independent STEP distance/witness acceptance.
Supports remain analytic and nonconvergence is explicit; annular/nonconvex bodies,
general trimmed curves, continuous motion and manufacturing-envelope clearance
remain open.

Geometric clearance now has a separate bounded native convex-planar query and
source-owned occurrence-pair obligation. `clearance_station.star` exercises nested
rotations and oblique jaws; independent STEP minimum-distance and witness checks
are required alongside native numerical and failed-publication tests. This advances
engineering validation beyond size-only stacks, while general curved/nonconvex queries,
whole-assembly pair traversal and continuous-motion collisions remain open.

M6 now also includes bounded worst-case linear dimensional stacks, referencing
native tolerance witnesses and enforcing the complete outward-rounded interval.
`bearing_clearance.star` adds a size-clearance requirement and read-only report to
the drawing workflow. Independent exact-rational comparisons, cache reuse, offline
replay and failed-generation preservation cover this increment. General spatial
clearance, statistical/correlated stacks, fit classes and GD&T remain open.

M6 dimensional tolerances now attach explicit nominal and signed deviations to
native linear/circular dimensions. `bearing_limits.star` adds bore, pin and pitch
requirements to a source-owned A4 sheet. Nominal/native agreement remains a
global build obligation independent of the authored manufacturing interval.
Geometry reuse, failure preservation and offline annotation replay are acceptance
requirements. Continue toward fit classes, GD&T, tolerance stacks, manufactured
inspection and PMI exchange; authored size limits do not close those gates.

M6 now includes source-owned `drawing_sheet` layouts of existing native section
views and their supported dimensions. `bearing_drawing.star` composes two
explicitly scaled views with a title/revision block and source/digest witnesses.
Build status `layout_validated` covers references and paper layout; publication
checks geometry/annotation fit before marking the sheet `published`. Overflow
preserves the prior generation. Tests cover revision-only geometry reuse and
offline captured-bundle replay. Projected hidden-line views, fits, GD&T and a
complete revised manufacturing release remain separate acceptance obligations.

M5 now includes native planar edge chamfers. The guide-block example chains six
local cuts and attaches inclined wear pads to a generated chamfer datum. Its
parameter corpus checks independent volume/centroid/inertia, while STEP checks
verify support planes, sections and assembly placement. This bounded operation
starts M5; the general edge-treatment and repair gates remain open.
Semantic `chamfer_faces` now connects persistent face references to a native
modeling consumer: original stock roles follow preceding cuts and rigid transforms
before selecting the edge. Cache and publication checks must reject broken intent,
including geometrically identical but unrelated anchors. General edge/vertex
selectors and curved edge treatments remain open.

Use the architecture's [delivery priorities](architecture.md#delivery-priorities-and-completion-authority)
to select and review increments across modeling, assembly scale, inspection,
constraints, manufacturing and automation. Report the supported domain and
evidence revision for each increment. Preserve separate proposed, implemented
and independently validated states; none alone declares commercial parity.

Enforce the architecture's [four graph boundaries](architecture.md#four-distinct-graphs)
in integration acceptance: camera changes affect display demand, placements reuse
part geometry, source edits invalidate affected feature descendants, and meshing
policy changes leave native engineering results unchanged. For a declared frame
fabrication workflow, M6 also requires revision-linked cut lists with tested
stock/material grouping and configuration-aware quantities.

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

Use the five [product acceptance priorities](architecture.md#product-acceptance-priorities)
when selecting work: trustworthy revisions, assembly scale, mechanical intent,
manufacturing evidence and source-driven automation. Every increment declares
which outcome it advances and an executable acceptance case. Keep the complete
drive-module release workflow as the integration gate; rendering milestones do
not close its modeling, solver or manufacturing obligations.

M5 repair acceptance must additionally measure cumulative drift against the
preserved source across chained and repeated repairs. Verify repeat-policy
stability and authorized thin-wall/fit/clearance limits; per-step tolerance
compliance alone does not pass this gate.
The first M6 exchange-fidelity sidecars bind STEP/STL preservation/loss declarations
to inspected native source and exported bytes. Source-authored required-field publication gates now reject unsupported, absent
and unknown semantics, including selection/display-only bypasses. Independent
semantic round-trip evidence and complete manufacturing release remain open.
STEP now preserves authored part numbers, descriptions and revisions while
retaining internal definition identities and shared geometry. Independent decoded
field checks cover duplicate part numbers, revision edits and nested occurrences;
material names and explicitly dimensioned density also have independent STEP
and XCAF checks, including density-only revisions with unchanged geometry.
Datum intent, PMI and full material specifications remain open.
M6 release policies declare required manufacturing semantics. Loss of required
units, tolerances, datum references, revisions or occurrence mappings blocks
release and is checked independently from exported solid validity.

| Milestone | Deliverable and example progression | Acceptance evidence | Status |
| --- | --- | --- | --- |
| M0: executable foundation | Recipes, orthogonal B-rep, validated builds, STEP/STL, HTTP inspection, source panel, watch. Plate → bracket → tray → fixture → vented enclosure | Analytic volume/bounds/topology checks, parameter edits, independent STEP imports, cache/publication/browser tests | Working bounded baseline; maintain continuously |
| M1: design identity and assembly model | Versioned part definitions, instances, rigid transforms, datums, semantic references, lineage. Fixtures become true component assemblies; repeated fasteners share one definition | One definition edit computes once; placement changes compute no new geometry; 500 committed reference/edit sequences have no silent retargeting | In progress: nested instances, named part datums with part/rigid-module attachments, shared assets/hierarchical STEP, scoped face-role queries and orthogonal lineage implemented; associative planar face datums implemented; broader selectors and downstream solid-modeling consumers remain |
| M2: general planar and analytic geometry | Planar profiles with holes, general rigid transforms, circles/arcs/cylinders, revolution, correct trim/seam handling and scoped Booleans. Angled fixture → drilled mounting bracket → shaft/collar → supported bearing housing | Independent formula/integral checks, thin/tangent/coincident adversarial cases, explicit unsupported inputs, analytic STEP checks | In progress: simple polygon outer profiles with simple nonconvex holes, exact rigid transforms of supported prisms, and bounded convex planar intersection with lineage implemented; analytic disk/annular extrusions, mixed polygon/circular through-hole prisms and their periodic seams implemented; full-turn piecewise-linear-profile revolution including regular conical frusta and conical poles implemented; simple concave line/arc path extrusion with circular holes and bounded parallel-cylinder union/intersection/through-cut and sequential contained circular through/blind bores, coaxial counterbores and conical countersinks in recognized prisms and circular/annular stock implemented; curved inner paths, arcs in revolved meridians, partial revolution, general Booleans and generalized trim/seam handling remain |
| M3: scalable inspection | Meshing interface, shared display assets, GPU instancing, metadata-first loading, bounded residency, LOD/cluster refinement. Parametric rack with thousands of repeated components | 10k-instance initial benchmark then 25k gate; zero exact loads for overview; fixed camera traces and stated hardware/error budgets | In progress: opt-in WebGL2 shared-mesh instancing, depth-tested picking, display-only publication and repeat/unique workload harness; selective verified mesh streaming, occurrence-BVH frustum culling and bounded cache and analytic whole-part LOD implemented; experimental bounded metadata paging with adaptive LOD also implemented; total decoded-memory validation, cluster LOD, part spatial indexes and hardware/detail-latency gates remain |
| M4: design constraints | 2D sketches, 3D rigid-joint graphs, residual/DOF/conflict reporting and branch stability. Dimensioned slope fixture → four-bar mechanism → constrained gearbox layout | Over/underconstrained cases, singular positions, edit continuation, joint residual bounds, collision distinct from constraint solving | In progress: bounded original Rust point sketch solver, ten constraint kinds including directed angles and circle-to-line tangency; separate linear/angular tolerances, polygon extrusion, local rank/residual/conditioning reports, independent constraint islands and atomic failure handling; curves, large connected systems, branch continuation, conflict minimization and nonlinear assembly solving remain; bounded prescribed revolute/prismatic forward kinematics has product/exchange acceptance; planar pin/bar closed-loop integration adds numerical solution and explicit branch admission |
| M5: demanding solid modeling | Sweeps/lofts, general curved intersections, chamfers/fillets, offsets/shelling, controlled repair. Filleted bearing housing → ribbed enclosure → swept manifold | Trimmed-surface validity, repair-deviation reports, failure minimization, broad edit corpus, independent exchange/query results | In progress: local equal-distance straight-edge chamfers on strictly convex planar solids, with complete face lineage and generated-face datums; curved/unequal chamfers, fillets, sweeps/lofts, offsets/shelling and general repair remain |
| M6: manufacturing deliverables | Associated drawings/sections, dimensions/datums/GD&T intent, materials, mass/inertia, BOM, analytic/assembly STEP and PMI support matrix. Releasable multi-part fixture with drawing pack | Stable annotation edits, material/quantity rollups, no LOD-dependent measurements, export fidelity/loss reports | In progress: part metadata, nested quantity BOM, native centroid/central inertia and density-weighted assembly queries and global recipe property assertions and revision-bound STEP/STL fidelity declarations and source-bound native planar-section JSON/SVG implemented; drawings, PMI, broader independent semantic round-trip evidence, principal axes and configuration policies remain |
| M7: team-scale engineering | Pinned releases, semantic review, isolated job service, artifact distribution, cancellation/recovery, revision/BOM adapters. Multi-contributor machine module | Reproducible revisions, isolation/quotas, concurrent publication, slow-network/offline recovery, audited artifact identity | Planned |

Completing M1 and extending the display-asset interface are the immediate
architectural priorities. Nested assemblies now share definitions and display
meshes. The opt-in WebGL2 viewer uses GPU instancing; Canvas remains a portable
fallback. The GPU client now also supports metadata-first selective mesh loading,
occurrence-BVH frustum culling and bounded geometry residency with visible proxies.
Analytic parts now refine through up to three native tessellation levels with
explicit unmet-detail reporting. The experimental paged inspector combines these
levels with verified occurrence pages. Total decoded-memory validation, within-part
spatial hierarchies, cluster LOD and hardware performance gates remain. Numerical kernel development continues in parallel through M2; GPU display work does not block headless geometry or substitute for it.

M1 instance transforms describe placement and display reuse. The M2 `transform`
operation now bakes rigid rotations and translations into exact geometry for
supported convex solids, simple-profile and mixed analytic prisms, cylinders,
tubes, and full-turn revolved solids; orthogonal multi-solid geometry additionally
supports exact axis permutations. It does not enable
general union/cut across rotated solids or modeling across assembly instances. `tilted_jig.star`
adds two transformed tools with cap/wall lineage assertions; manufacturing files
contain their transformed native coordinates.

The next M4 mechanism integration solves bounded planar pin/bar closure with the
original numerical solver, then binds solved frames to rigid occurrences. Its
four-bar fixture passes native 27-case closure/mass/section checks, rejected-edit
publication checks and three independently imported STEP configurations.
This advances closed-loop mechanics beyond prescribed coordinates. Keep general
spatial constraints, continuation through singularities and collision checks open;
explicit initial guesses and signed branch admission do not establish them.

M4 now includes source-owned sampled continuation: warm starts, adaptive drive/pin
step bounds, explicit branch admission, charged solve budgets and preserved failure
prefixes. The motion-four-bar fixture reuses locked source and publishes a recorded
layout inspector plus source-bound JSON. Its acceptance checks sampled closure,
round trips, static-geometry reuse and failure-preserving publication. Continuous
branch certification, singularity crossing, physical interference and dynamics
remain open. Sample playback never executes modeling calls.

The continuous-translation increment extends geometric clearance to complete
straight-line right-occurrence travel, with adaptive native distance certificates
and explicit work/uncertainty limits. `translation_clearance.star` requires
whole-path expected minima and interior-collision rejection, including collisions
missed by endpoint/midpoint sampling. This advances mechanical validation without
closing general rotating-motion clearance or the R3 release gate.

The fixed-axis rotation increment extends this certificate to a signed angular
path around a source-owned local pivot and axis. `rotation_clearance.star` orbits
a tool past radial and overhead guards and includes an interior collision hidden
by endpoint/midpoint sampling. Its radius/speed bounds and independent orbit/STEP
checks share the existing work, failure-publication and source-replay contracts.
General coupled motion and the complete R3 release gate remain open.

## Resumed integration — 2026-10-12

The user resumed work toward an integrated bearing-supported drive-module
revision demonstration. Independent rotation acceptance now passes 15
configurations/30 paths, 183 covering intervals, 138 imported STEP poses and four
publication-preserving failures. The read-only browser report also passes its
source/scope checks. This closes the pending rotation increment acceptance, not
the commercial R3 gate.

The next product evidence joins a reviewable shaft-diameter/mounting-pitch source
patch, before/after artifacts, native engineering checks, drawings/BOM/STEP,
rejected-edit recovery and captured-source replay. Simplified fastener envelopes
and a drive-disc blank must stay explicit; axial retention and torque transfer
are not established by positive clearance or by the display alone.

The finite shaft-keyseat increment adds a curved-boundary machining operation
needed by the drive module: a rectangular pocket through the cylindrical exterior
with intact journals at both ends. The isolating `keyed_shaft.star` fixture and
independent cylinder-minus-box oracle validate the geometry before assembly use.
The matching internal through-keyway increment preserves the annular hub's outer
wall and adds a keyed bore with exact cylindrical and planar supports.
`keyed_coupling.star` combines these two cuts with a separate key and a transverse
engagement drawing; independent part-intersection and face-gap checks are required
before accepting the fixture. This establishes nominal geometric engagement,
not rated torque transfer. Next geometry obligations include
composition with a stepped shaft/retention groove; the
mechanical layout also needs flanged bushings, a thrust washer/retainer and a
reviewed endplay stack. Do not replace these with a visual key overlapping uncut
material, an unattached collar, or an unvalidated friction claim.

The native coaxial trim-witness query now preserves the integrated module's
clearance obligations: a containing round-stock distance supplies a lower bound,
while points certified to survive the actual cuts supply the upper bound. Reject
cases without a surviving witness or a sufficiently tight interval.
`drive_module_keyed.star` combines the real shaft seat, hub keyway and separate
key with all seven earlier clearance checks. A complete-circle section certificate
retains shaft diameter dimensions across split native wall descendants, without
accepting partial or ambiguous sections. The next mechanical increment is true
axial retention with an independently checked endplay stack; key load capacity,
standardized fits and manufacturing readiness remain separate release obligations.

## Next concrete implementation increments

Published generations now carry a payload digest/size inventory, with a separate
manifest schema and read-only `cad verify` command. Externally pinned manifest
digests detect changes to the complete generation. Continue M7 toward a versioned
capability-evidence ledger. Its first engineering case now records actual native
and independent counterbore checks, retained models and full-moment oracle reports,
source snapshots, tested binaries and result digests. CI collects this case and
tests altered-evidence rejection. Continue toward hermetic replay, hardware-pinned
performance records and the full drive-module release pack; internally consistent
bytes alone do not establish trusted execution or complete a manufacturing release.

The first M3a increment now streams whole-part analytic LOD, preserving current
surfaces while finer assets load and exposing unmet detail under resource or
precision limits. `spindle_line.star` combines tapered housing variants with
shared drilled bases, shafts and rings. The experimental paged inspector now integrates these shared mesh levels with
bounded occurrence loading. Continue toward total decoded-memory measurement,
cluster refinement and hardware/detail-latency acceptance; a working part-LOD
viewer does not close those gates.

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

Apply the architecture's [resource ownership contract](architecture.md#resource-ownership-across-the-product)
across simultaneous build, inspection and export workloads; neither a display
cache cap nor a mesh-cache retention cap establishes bounded process memory.
Keep [GPU execution boundaries](architecture.md#gpu-and-portability) explicit:
optional analytic rendering and compute acceleration require their own fidelity,
failure and backend-identity evidence. They cannot replace native modeling or
manufacturing gates and do not block the CPU-only headless product.
M3 progressive publication must pin every display revision to one immutable
design generation. M1 configuration work must implement explicit suppression
participation and reject live references to suppressed occurrences. Hosted M7
delivery must authorize the complete artifact/dependency graph in project scope;
content-addressed storage alone does not establish tenant isolation.

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
CCW simple-profile, half-turn-arc, strict circular-hole separation and numerical
limits. Concave outer profiles and clockwise recesses now extend this domain;
curved inner paths, arc meridians and general curved Booleans remain future work. Native engineering assertions remain build gates even for
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
   contract and general planar Booleans alongside the scoped parallel-cylinder branch;
   neither establishes general curved Boolean support.
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
The next implemented M4 increment adds equal-length, parallel, perpendicular
and directed-angle equations. Schema-3 reports separate millimetre and radian
residuals; angular tolerance is independently fixed at 1e-9 radians. Mixed
normalized-linear/radian Jacobian conditioning is not a coordinate error bound.
Zero/unresolved directions and angular residuals within 1e-8 radians of a wrap
cut fail explicitly; candidate steps cannot silently cross that cut.
`constrained_angle_fixture.star` combines an angle/perpendicular triangle with
an equal-leg, parallel-edge trapezoid deck and associative side pads: nine
occurrences, five definitions, span 50..80 mm, angle 15..45 degrees and deck top
width 18..30 mm (defaults 60/30/24). Independent trigonometric native checks over
27 configurations pass, as do nine browser cases and three independent STEP
oracle cases. Conflicting angles return separately unit-labelled residuals and
preserve all artifacts of the previous valid generation.
The circular-hole increment connects solved centre/rim handles to native
cylindrical through-holes in extrusion. `constrained_drill_jig.star` adds a
six-occurrence, three-definition assembly; independent 27-case checks cover
plate moments, assembly material properties and parameter/cache behavior.
This is point-handle circle construction; independent circle variables,
independent arc variables and circle-to-circle tangency remain open.
Circle-to-line tangency now has explicit directed-line side selection and
linear residual/error budgets. `tangent_bore_fixture.star` adds two opposed
wedge-positioned bores to a seven-occurrence assembly, with 27 native cases,
three independent STEP cases and nine browser cases (144 visibility rays).
Advance toward independent arc variables, circle-to-circle tangency and explicit continuation
before claiming general sketch coverage; nonlinear rigid assembly constraints are a separate
solver milestone. Prescribed revolute/prismatic coordinates now compose through
the acyclic datum-attachment graph, with finite inclusive limits, global resource
checks and residual reports. The jointed linear-stage fixture passes 27 native
configurations and three independent STEP/section configurations, including
nonidentity root poses. Failed coordinate edits preserve publication and motion
edits compute no new solids. This does not solve loops or certify collisions.

The curved-profile increment connects point constraints to native convex
line/arc outer boundaries with circular holes. `constrained_capsule_link.star`
combines two shared curved link plates, two spacers and two stepped pins. Its
acceptance starts from perturbed guesses and compares nominal capsule moments,
parameter edits, native validation, independent STEP and displayed bores/arcs.
A sketch meeting its requested tolerance must still pass the stricter native
arc-support consistency checks before publication. Bounded residual refinement
preserves the requested acceptance policy and retains only fully certified
snapshots; it never projects coordinates or weakens native validation.

Independent constraint islands extend the total sketch envelope to 256 points
and 1,024 constraints while retaining local 32-point/128-constraint limits.
Schema-3 reports preserve every group's status and membership; failures block
publication even when other groups converge. `modular_bore_plate.star` exercises
30 bores and 64 points in a 33-occurrence tooling panel. Connected large-system
solving, sparse elimination and incremental island result caching remain future
work; the implementation does not claim them from disconnected-group scaling.
Native 27-case, three-case independent STEP and nine-case browser acceptance
pass. Certified tessellation is now reused across publication consumers through
a bounded generation cache; the 30-bore fixture measures its effect separately
from constraint solving. Continue toward bounded exact-body residency and display
preprocessing budgets; a cache retention cap is not a full job memory bound.

The concave-profile increment uses `split_bearing_block.star`: two shared jaws
with inward semicircular seats, a stepped shaft and four sleeves. Native 27-case
acceptance checks independent volume, centroid, full volume/mass inertia and
parameter invalidation. Analytic trimmed-boundary separation and winding replace
the convex halfspace restriction without using tessellation for native validity.
This is a step toward the supported bearing housing in M2; general curved
Booleans, fillets, fits and manufacturing release remain separate obligations.

The parallel-cylinder increment uses `twin_lobe_fixture.star`: a union base,
two crescent inserts, a lens insert and two shared bosses. Native circle
arrangements retain source-wall and cap lineage; independent segment moments
cover 27 parameter configurations and assembly mass/inertia. Tangent contact,
partial-depth cuts and unresolved cap offsets preserve the last valid publication.
This is a bounded M2 Boolean branch. Extend operand composition and general
intersection/trim handling before claiming curved Boolean closure.

Sequential drilling now composes circular tools with existing polygon/arc-prism
stock, preserving original face identities and each tool wall's lineage.
`machined_bearing_support.star` applies four successive bores to a concave jaw,
then assembles eleven occurrences from three definitions. Independent 27-case
moments, selective invalidation and failed-publication obligations cover this
machining sequence. The native blind-bore extension adds top/bottom flat-floor
sockets with existing face identity, exact mass properties and analytic export.
`blind_bore_pallet.star` combines six top sockets, two underside sockets and four
through-holes, then nests two shared tooling stations (26 occurrences, four
definitions). Independent 27-case full moments, STEP/STL checks, floor picking and
failed-publication obligations cover this composition. Advance toward crossing
cuts and general pockets; separated cylindrical groups do not establish general
material-removal capability. Coaxial counterbores now use native cylindrical
sections and annular shoulders, with overlap lineage and independent union-volume
integration. `counterbored_fixture.star` combines eighteen machining features,
twelve shoulder datums and a mirrored 32-occurrence fastening stack. Full native
and material moments across 27 configurations, independent exchange checks and
shoulder-picking checks cover this composition. Noncoaxial intersections remain future work. Conical countersinks now extend
the axial machining boundary as described below.
Native circular and annular stock now enter the same machining sequence through
an exact outer-cylinder boundary split with composed face lineage.
`round_bearing_cartridge.star` adds a mirrored bearing assembly with socketed rods,
center bore and counterbored fasteners. Its independent moments, exchange and
viewer cases extend the M2 workflow; they do not close general Boolean or
manufacturing-release gates.


The AI-native authoring interface now exposes a schema-2 capability
catalog through the CLI and Rust SDK, with JSON IR recipe, feature, assembly and
engineering-check argument shapes, geometry domains, exclusions, limits and
implementation identities. Focused operation/type views include nested inline
types and retain logical references as IDs. Parser alignment,
example compilation, side-effect-free discovery and source-correction/replay
acceptance cover this boundary. Native operation failures identify the failing
feature as structured data. This advances automation infrastructure; it does not
close hosted M7 services, release-evidence policies or actual AI design acceptance.

## Current mechanical increment: shaft retention

The keyed-drive workflow now has a separate `retained_shaft.star` fixture with
native stepped-shaft shoulders, a retaining groove, a finite keyseat, opposed
flanged bushes, a washer and an open C-ring. Native clearance preserves the
actual stock meridian. Independent exchange checks cover three installed
configurations and six isolated scale/pose cases, full moments, native topology,
mesh closure, part interference and nominal axial endplay.

Directed native datum dimensions now make axial endplay a source-authored
obligation, with numerical bounds separate from authored size tolerances.
`drive_module_retained.star` integrates both shaft and hub nominal retention:
24 occurrences, 11 part definitions, ten native axial dimensions and two endplay
stacks. The revision walkthrough rejects excessive clearance and endplay,
preserves the valid generation, and replays captured source without a cache.
The rotating-drive extension now prescribes nine co-moving occurrences and
certifies eleven named separation pairs for a complete signed turn using common
rigid-motion invariance and native stationary symmetry. Its phase inspector is
source-bound and read-only. Installation paths, unlisted contacts and engineering
load assumptions remain open, alongside the drawing and scale gates.
Bush anchorage, ring installation, standardized fits and load capacity remain
explicit engineering obligations. These increments advance M2/M4/M6; all five
commercial release gates remain open.

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


The conical machining increment uses `countersunk_bearing_cover.star`: two
annular covers with six tapered seats and three locating sockets each, plus
shared flush fastener envelopes. It extends M2 with linear-radius axial sections,
continuous shared cone/cylinder rings and analytical intersection breakpoints.
Its independent native/STEP moments, mesh closure, semantic-edit obligations and
browser picking must pass alongside the existing cylindrical machining corpus.
The evidence ledger records this composition as `machining.countersunk-bearing-cover`;
it does not close general curved Booleans or M6 manufacturing drawings.

## Meaning of completion

The current orthogonal product and convex polygon increment close neither the commercial-platform
goal nor the kernel roadmap. A credible commercial-competition release requires
a published supported-workflow matrix, measured assembly and regeneration
behavior, reliable manufacturing exchange/deliverables, reproducible source-driven
workflows, and completed gates for its declared scope. Unsupported cases remain
explicit. The overall project goal stays active as these milestones are built.

### Locked source-library increment

M7 now includes explicitly pinned local Starlark module bundles, transitive load
validation, captured offline replay, dependency-aware watch and module inspection.
The modular linear-stage example reuses beveled rails and drilled carriages and
checks mass/sections across parameter revisions. This advances source-driven team
and agent workflows; remote package resolution, source-span navigation and hosted
worker isolation remain open. Module-only edits must rebind publication evidence
even when the compiled geometry is unchanged.

The local source-library workflow now includes explicit `cad lock` creation and
updates with transitive discovery and change reporting. Lock generation does not
execute a design; native validation remains a separate build obligation. Package
publication also retains the previous complete archive until a new archive has
been staged and checked, with forced-failure and concurrent-reader regressions.
