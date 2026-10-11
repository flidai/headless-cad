// Independent authored-corner oracle for occurrence BVH traversal. This verifies
// correctness and measured pruning, not GPU frame-rate or a hardware claim.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const execute = promisify(execFile);
const root = path.resolve(__dirname, '../..');
const binary = process.env.CAD_BIN || path.join(root, 'target/debug/cad');
const side = 160;
const source = `# BVH acceptance: repeated meshes, different definitions, and rotated occurrences.
def grid():
    parts = []
    for i in range(8):
        parts.append(part(id = "p_" + str(i), body = extrude(id = "e_" + str(i), profile = rectangle(id = "r_" + str(i), width = 8 + i, depth = 6), height = 4)))
    placements = []
    rotations = [[0,0,0,1], [0,0,0.7071067811865475,0.7071067811865476], [0.3826834323650898,0,0,0.9238795325112867], [0,0.3826834323650898,0,0.9238795325112867]]
    for row in range(${side}):
        for col in range(${side}):
            index = row * ${side} + col
            placements.append(instance(id = "cell_" + str(index), part = parts[index % 8], offset = [col * 100, row * 100, (index % 3) * 20], rotation = rotations[index % 4]))
    return placements
assembly_output(name = "grid", assembly = assembly(id = "grid", instances = grid()))
`;
// Transform all eight corners with authored trigonometric matrices; do not use
// the renderer's quaternion transforms, cached centers, support functions or BVH.
function corners(index) {
  const t = [(index % side) * 100, Math.floor(index / side) * 100, (index % 3) * 20];
  const result = [];
  for (const x of [0, 8 + index % 8]) for (const y of [0, 6]) for (const z of [0, 4]) {
    let p;
    switch (index % 4) {
      case 0: p = [x,y,z]; break;
      case 1: p = [-y,x,z]; break;
      case 2: p = [x,(y-z)*Math.SQRT1_2,(y+z)*Math.SQRT1_2]; break;
      case 3: p = [(x+z)*Math.SQRT1_2,y,(z-x)*Math.SQRT1_2]; break;
    }
    result.push(p.map((v,a) => v+t[a]));
  }
  return result;
}
let browser, temporary, server;
(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-bvh-check-'));
  const recipe = path.join(temporary, 'grid.star'), output = path.join(temporary, 'out');
  await fs.writeFile(recipe, source);
  await execute(binary, ['build', recipe, '--display-only', '--out-dir', output], { maxBuffer: 16 * 1024 * 1024 });
  const scene = JSON.parse(await fs.readFile(path.join(output, 'grid.stream.json'), 'utf8'));
  assert.equal(scene.instances.length, side * side);
  assert.equal(Object.keys(scene.mesh_assets).length, 8);
  const nativeTopFaces = new Map();
  for (const [hash, asset] of Object.entries(scene.mesh_assets)) {
    const mesh = JSON.parse(await fs.readFile(path.join(output, asset.url), 'utf8'));
    const top = mesh.faces.find(face => face.normal[0] === 0 && face.normal[1] === 0 && face.normal[2] === 1);
    assert.ok(top, 'Authored box has a native top face');
    nativeTopFaces.set(hash, top.id);
  }
  const worldCorners = scene.instances.map(instance => corners(Number(instance.id.slice(5))));
  const margin = Math.max(Math.hypot(...scene.bounds[1].map((v,a)=>v-scene.bounds[0][a])) / 2, 1e-9) * 1e-12;
  const requests = [], failures = [], errors = [];
  server = http.createServer(async (request, response) => {
    const name = new URL(request.url, 'http://localhost').pathname.slice(1);
    if (name === 'favicon.ico') { response.writeHead(204); response.end(); return; }
    requests.push(name);
    try {
      const bytes = await fs.readFile(path.join(output, name));
      response.writeHead(200, { 'Content-Type': name.endsWith('.html') ? 'text/html' : 'application/json' });
      response.end(bytes);
    } catch (error) { failures.push(error.message); response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  let comparisons = 0, picks = 0;
  const pruning = [];
  function oracle(camera) {
    const axes = [camera.right, camera.up, camera.eye], extents = [camera.halfWidth, camera.halfHeight, camera.depthRange];
    return worldCorners.flatMap((points,index) => {
      const outside = axes.some((axis,a) => {
        const distances = points.map(p => p.reduce((sum,v,k)=>sum+(v-camera.target[k])*axis[k],0));
        return distances.every(v=>v>extents[a]+margin) || distances.every(v=>v< -extents[a]-margin);
      });
      return outside ? [] : [index];
    });
  }
  async function check(page, label) {
    const camera = await page.evaluate(() => { cadGpuTest.render(); return cadGpuTest.snapshot(); });
    assert.deepEqual(camera.visibleIndices, oracle(camera), label);
    assert.equal(new Set(camera.visibleIndices).size, camera.visibleIndices.length, 'No duplicate occurrence candidates');
    comparisons++;
    return camera;
  }
  for (const mode of ['gpu','stream']) {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.on('pageerror', e => errors.push(e.message));
    const start = requests.length;
    const initial = { yaw: -Math.PI/2, pitch: Math.PI/2, target: [4,3,2], halfHeight: 25 };
    await page.goto(`${base}/grid.${mode}.html#camera=${encodeURIComponent(JSON.stringify(initial))}`);
    await page.waitForFunction(() => window.cadGpuStats?.ready || window.cadGpuStats?.errors?.length);
    await page.evaluate(() => cadGpuTest.whenIdle());
    const focused = await check(page, `${mode}: focused`);
    assert.deepEqual(focused.visibleIndices, [scene.instances.findIndex(i=>i.id==='cell_0')]);
    const stats = await page.evaluate(() => ({...cadGpuStats}));
    assert.equal(stats.ready, true);
    assert.deepEqual(stats.errors, []);
    assert.ok(stats.bvhNodeCount > 1 && stats.bvhLeafCount > 1 && stats.bvhMaxDepth > 1);
    assert.ok(stats.cullOccurrenceTests < side*side/8, `Focused ${mode} query did not prune occurrences: ${JSON.stringify(stats)}`);
    assert.ok(stats.cullNodeTests < stats.bvhNodeCount / 2);
    assert.ok(stats.cullPrunedNodes > 0);
    pruning.push({ mode, occurrences: scene.instances.length, tested: stats.cullOccurrenceTests, nodes: stats.cullNodeTests });
    if (mode === 'stream') {
      const fetched = requests.slice(start).filter(name=>name.endsWith('.display.json'));
      assert.deepEqual(fetched, [scene.mesh_assets[scene.definitions.p_0.mesh].url], 'Cold focused traversal fetches only visible display assets');
    }
    // Deterministic arbitrary orbit, pan and zoom queries, including wide views.
    for (let i=0;i<28;i++) {
      const camera = { yaw: -2.7+i*.37, pitch: -.9+(i%7)*.29, target: [(i*617)%(side*100)-100,(i*1291)%(side*100)-100,(i%5)*20], halfHeight: [19,85,300,1200,side*100][i%5] };
      await page.evaluate(camera=>cadGpuTest.setCamera(camera), camera);
      await check(page, `${mode}: orbit/pan/zoom ${i}`);
    }
    await page.evaluate(()=>cadGpuTest.setCamera({target:[-100000,-100000,-100000],halfHeight:25}));
    const empty = await check(page, `${mode}: wholly offscreen`);
    assert.deepEqual(empty.visibleIndices, []);
    // All-visible root, plus exact and neighboring frustum-plane contact.
    await page.evaluate(()=>{cadGpuTest.setView('top');cadGpuTest.fit();});
    const all = await check(page, `${mode}: fit`);
    assert.equal(all.visibleIndices.length, side*side);
    await page.evaluate(()=>{cadGpuTest.setView('top');cadGpuTest.setCamera({halfHeight:25,target:[4,3,2]});});
    const top = await check(page, `${mode}: reset`);
    for (const displacement of [-1e-5,0,1e-5]) {
      await page.evaluate(({halfWidth,displacement})=>cadGpuTest.setCamera({target:[-halfWidth+displacement,3,2]}), {halfWidth:top.halfWidth,displacement});
      const boundary = await check(page, `${mode}: clip boundary ${displacement}`);
      const contains = boundary.visibleIndices.includes(scene.instances.findIndex(i=>i.id==='cell_0'));
      assert.equal(contains, displacement>=0, 'Touching bound retained; definitely outside bound pruned');
    }
    // Native occurrence/local-face identity and source survive candidate sorting,
    // camera moves and sharing one display mesh across many occurrences.
    for (const authored of [0,Math.floor(side*side/2)+1,side*side-2]) {
      const points = corners(authored), center = [0,1,2].map(a=>points.reduce((s,p)=>s+p[a],0)/8);
      await page.evaluate(center=>{cadGpuTest.setView('top');cadGpuTest.setCamera({target:center,halfHeight:25});cadGpuTest.render();}, center);
      await page.evaluate(()=>cadGpuTest.whenIdle());
      await check(page, `${mode}: pick ${authored}`);
      const result = await page.evaluate(center=>{const [x,y]=cadGpuTest.project(center);return cadGpuTest.pick(x,y);}, center);
      assert.equal(result?.instance, scene.instances.findIndex(i=>i.id===`cell_${authored}`));
      const instance = scene.instances[result.instance];
      assert.equal(result.face, nativeTopFaces.get(scene.definitions[instance.part].mesh), 'Authored top face identity survives BVH ordering');
      assert.match(await page.locator('#selection').textContent(), new RegExp(`cell_${authored}`));
      assert.equal(await page.evaluate(()=>recipeContext.source), source);
      picks++;
    }
    await page.close();
  }
  assert.equal(requests.some(name=>/\.(brep\.json|step|stl)$/.test(name)), false, 'Inspection does not fetch exact geometry');
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, []);
  console.log(JSON.stringify({occurrences:side*side,unique_meshes:8,independent_visibility_comparisons:comparisons,stable_picks:picks,pruning,exact_geometry_downloads:0}));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
  if(browser)await browser.close();
  if(server)await new Promise(resolve=>server.close(resolve));
  if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
