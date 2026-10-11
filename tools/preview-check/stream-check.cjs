// Adversarial local-network tests for display streaming. No exact B-rep fetches.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const crypto = require('node:crypto');
const execute = promisify(execFile);
const root = path.resolve(__dirname, '../..');
const binary = process.env.CAD_BIN || path.join(root, 'target/debug/cad');
const source = `# Streaming source snapshot; </script><img src=x onerror="window.injected=true">
size = param(name = "size", default = 8)
def modules():
    result = []
    for i in range(6):
        p = part(id = "part_" + str(i), body = extrude(id = "body_" + str(i), profile = rectangle(id = "profile_" + str(i), width = size + i, depth = 6), height = 4))
        result.append(instance(id = "cell_" + str(i), part = p, offset = [i * 100, 0, 0]))
        result.append(instance(id = "rotated_" + str(i), part = p, offset = [i * 100 + 20, 0, 0], rotation = [0, 0, 0.7071067811865475, 0.7071067811865476]))
    return result
assembly_output(name = "cells", assembly = assembly(id = "cells", instances = modules()))
`;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, message, timeout = 15000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) { if (await fn()) return; await sleep(25); }
  throw Error(`Timed out: ${message}`);
}
let browser, temporary, server;
(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-stream-check-'));
  const recipe = path.join(temporary, 'cells.star'), output = path.join(temporary, 'out');
  await fs.writeFile(recipe, source);
  await execute(binary, ['build', recipe, '--display-only', '--out-dir', output], { maxBuffer: 8 * 1024 * 1024 });
  const scene = JSON.parse(await fs.readFile(path.join(output, 'cells.stream.json'), 'utf8'));
  assert.equal(scene.schema_version, 2);
  assert.deepEqual(scene.meshes, {});
  const assets = Object.entries(scene.mesh_assets);
  assert.equal(assets.length, 6);
  const localMeshes = new Map();
  for (const [hash, asset] of assets) {
    const bytes = await fs.readFile(path.join(output, asset.url));
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), asset.sha256);
    assert.equal(bytes.length, asset.byte_length);
    localMeshes.set(hash, JSON.parse(bytes));
  }
  const requests = [], pending = new Map();
  let gated = true, active = 0, peak = 0, requestId = 0, corrupt = null, missing = null, oversized = null, oversizedClosed = 0;
  const failures = [];
  server = http.createServer(async (request, response) => {
    const name = new URL(request.url, 'http://localhost').pathname.slice(1);
    if (name === 'favicon.ico') { response.writeHead(204); response.end(); return; }
    const token = ++requestId;
    const isMesh = assets.some(([, asset]) => asset.url === name);
    requests.push(name);
    if (isMesh) { active++; peak = Math.max(peak, active); response.once('close', () => active--); }
    try {
      if (isMesh && gated) await new Promise(resolve => { pending.set(token, { name, resolve }); response.once('close', resolve); });
      pending.delete(token);
      if (response.destroyed) return;
      if (name === missing) { response.writeHead(404); response.end('Missing display mesh'); return; }
      let bytes = await fs.readFile(path.join(output, name));
      if (name === corrupt) bytes = Buffer.from(bytes.toString().replace('"faces"', '"facet"'));
      response.writeHead(200, { 'Content-Type': name.endsWith('.html') ? 'text/html' : 'application/json', 'Cache-Control': 'no-store' });
      if (name === oversized) {
        // Exact declared bytes, then an excess chunk on a never-ending body.
        // The client must cancel the reader, not merely release its lock.
        response.once('close', () => oversizedClosed++);
        response.write(bytes);
        setTimeout(() => { if (!response.destroyed) response.write(' '); }, 20);
        return;
      }
      response.end(bytes);
    } catch (error) { failures.push(error.message); response.writeHead(404); response.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const url = `http://127.0.0.1:${server.address().port}/cells.stream.html`;
  browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const errors = [];
  async function newPage(ignoreAbort = false, fragment = '') {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    page.on('pageerror', error => errors.push(error.message));
    if (ignoreAbort) await page.addInitScript(() => {
      // Simulate a transport completion racing cancellation. Correctness must
      // depend on request-generation checks, not AbortController alone.
      const fetchOriginal = window.fetch.bind(window);
      window.fetch = (input, options) => fetchOriginal(input, String(input).includes('.display.json') ? { ...options, signal: undefined } : options);
    });
    await page.goto(url + fragment, { waitUntil: 'domcontentloaded' });
    await page.waitForFunction(() => window.cadGpuStats?.ready);
    return page;
  }
  async function focus(page, cell) {
    await page.evaluate(cell => { cadGpuTest.setView('top'); cadGpuTest.setCamera({ target: [cell * 100 + 10, 5, 2], halfHeight: 15 }); cadGpuTest.render(); }, cell);
  }
  async function idle(page) {
    await page.evaluate(() => Promise.race([cadGpuTest.whenIdle(), new Promise((_, reject) => setTimeout(() => reject(Error('Streaming failed to become idle')), 15000))]));
  }
  async function pick(page, id, expectProxy = false) {
    const index = scene.instances.findIndex(instance => instance.id === id);
    const instance = scene.instances[index], number = Number(id.split('_')[1]);
    const point = id.startsWith('rotated') ? [number * 100 + 17, (8 + number) / 2, 4] : [number * 100 + (8 + number) / 2, 3, 4];
    const result = await page.evaluate(point => { const [x, y] = cadGpuTest.project(point); return cadGpuTest.pick(x, y); }, point);
    assert.equal(result?.instance, index, `Occurrence ID for ${id}: ${JSON.stringify(result)}`);
    if (expectProxy) {
      assert.equal(result.face, -1, 'A proxy must not claim a local geometry face');
      assert.match(await page.locator('#selection').textContent(), /proxy|not loaded/i);
    }
    else {
      const face = localMeshes.get(scene.definitions[instance.part].mesh).faces.find(face => face.id === result.face);
      assert.deepEqual(face?.normal, [0, 0, 1], `Local top face for ${id}`);
    }
    return result;
  }
  const page = await newPage(true);
  await until(() => pending.size === 4, 'four bounded initial fetches');
  assert.equal(peak, 4);
  await focus(page, 0);
  await pick(page, 'cell_0', true);
  await focus(page, 5);
  await pick(page, 'rotated_5', true);
  await focus(page, 0);
  const releaseOrder = [...pending.values()].reverse();
  gated = false;
  for (const request of releaseOrder) { request.resolve(); await sleep(20); }
  await idle(page);
  await pick(page, 'cell_0');
  await pick(page, 'rotated_0');
  assert.equal(await page.evaluate(() => recipeContext.source), source);
  assert.equal(await page.evaluate(() => !!window.injected), false);
  async function cullingOracle(page) {
    return page.evaluate(() => {
      const camera = cadGpuTest.snapshot(), expected = [];
      const dot = (a, b) => a.reduce((sum, v, i) => sum + v * b[i], 0);
      for (let index = 0; index < compactScene.instances.length; index++) {
        const id = compactScene.instances[index].id, cell = Number(id.split('_')[1]);
        const corners = [];
        for (const x of [0, 8 + cell]) for (const y of [0, 6]) for (const z of [0, 4]) {
          // Authored quarter-turn evaluated analytically, not runtime matrices.
          const world = id.startsWith('rotated') ? [cell * 100 + 20 - y, x, z] : [cell * 100 + x, y, z];
          const relative = world.map((value, axis) => value - camera.target[axis]);
          corners.push([dot(relative, camera.right), dot(relative, camera.up), dot(relative, camera.eye)]);
        }
        const extents = [camera.halfWidth, camera.halfHeight, camera.depthRange];
        const outside = extents.some((extent, axis) => corners.every(p => p[axis] > extent + 1e-8) || corners.every(p => p[axis] < -extent - 1e-8));
        if (!outside) expected.push(index);
      }
      for (const index of expected) if (!camera.visibleIndices.includes(index)) throw Error(`False cull of occurrence ${index}`);
      return { expected, actual: [...camera.visibleIndices].sort((a,b)=>a-b), resident: camera.residentKeys };
    });
  }
  const focused = await cullingOracle(page);
  assert.equal(focused.expected.length, 2);
  assert.deepEqual(focused.actual, focused.expected);
  // A box touching the frustum plane remains a candidate: no strict-inequality
  // culling may remove potentially visible geometry at a clip boundary.
  await page.evaluate(() => { const c = cadGpuTest.snapshot(); cadGpuTest.setCamera({ target: [-c.halfWidth, 3, 2] }); cadGpuTest.render(); });
  const boundary = await cullingOracle(page);
  assert.ok(boundary.expected.includes(scene.instances.findIndex(instance => instance.id === 'cell_0')));
  await focus(page, 0);
  await idle(page);
  const budget = Math.max(...assets.map(([, asset]) => asset.byte_length + asset.gpu_bytes)) + 128;
  assert.ok(budget < 2 * Math.min(...assets.map(([, asset]) => asset.byte_length + asset.gpu_bytes)));
  await page.evaluate(budget => cadGpuTest.setBudget(budget), budget);
  await idle(page);
  const firstAsset = scene.mesh_assets[scene.definitions.part_0.mesh].url;
  const countFirst = () => requests.filter(name => name === firstAsset).length;
  const before = countFirst();
  for (const cell of [4, 1, 0]) {
    const requestStart = requests.length;
    await focus(page, cell);
    const loadingBudget = await page.evaluate(() => cadGpuStats.residentBytes + cadGpuStats.reservedBytes);
    assert.ok(loadingBudget <= budget, 'In-flight reservations exceeded resident budget');
    await idle(page);
    await pick(page, `cell_${cell}`);
    await pick(page, `rotated_${cell}`);
    const wantedUrl = scene.mesh_assets[scene.definitions[`part_${cell}`].mesh].url;
    const meshRequests = requests.slice(requestStart).filter(name => assets.some(([, asset]) => asset.url === name));
    assert.ok(meshRequests.length >= 1);
    assert.ok(meshRequests.every(name => name === wantedUrl), 'Hidden mesh requested by focused view');
    const state = await page.evaluate(() => ({ stats: cadGpuStats, camera: cadGpuTest.snapshot() }));
    assert.ok(state.stats.residentBytes + state.stats.reservedBytes <= budget, JSON.stringify(state.stats));
    assert.ok(state.stats.residentMeshCount <= 1);
    assert.ok(state.stats.loadingCount <= 4);
    assert.equal(state.camera.residentKeys.length, 1);
    const culling = await cullingOracle(page);
    assert.deepEqual(culling.actual, culling.expected);
  }
  assert.ok(countFirst() > before, 'Evicted asset was fetched again on returning to A');
  // Two visible definitions exceed the one-mesh budget. Selecting the unloaded
  // occurrence must refine that occurrence, without silently choosing a face.
  await page.evaluate(() => { cadGpuTest.setCamera({ target: [60, 5, 2], halfHeight: 70 }); cadGpuTest.render(); });
  await idle(page);
  await pick(page, 'cell_1', true);
  await idle(page);
  await pick(page, 'cell_1');
  const selectedResidency = await page.evaluate(() => cadGpuTest.snapshot().residentKeys);
  assert.deepEqual(selectedResidency, [scene.definitions.part_1.mesh]);
  const requestsBeforeStay = requests.length;
  await page.evaluate(() => { cadGpuTest.render(); cadGpuTest.render(); });
  await idle(page);
  assert.equal(requests.length, requestsBeforeStay, 'Settled residency must not refetch/thrash');
  corrupt = scene.mesh_assets[scene.definitions.part_2.mesh].url;
  const damaged = await newPage();
  await idle(damaged);
  const damagedState = await damaged.evaluate(() => ({ ready: cadGpuStats.ready, failures: cadGpuStats.assetErrors }));
  assert.equal(damagedState.ready, true, 'Asset failure must preserve usable display');
  assert.ok(Object.keys(damagedState.failures).length >= 1, 'Tampered asset was not rejected');
  await focus(damaged, 2);
  await idle(damaged);
  await pick(damaged, 'cell_2', true);
  assert.match(Object.values(damagedState.failures).join(' '), /hash|digest|sha|integrity/i);
  assert.match(await damaged.locator('#stream-status').textContent(), /failed verification|unavailable/i);
  await focus(damaged, 3);
  await idle(damaged);
  await pick(damaged, 'cell_3');
  corrupt = null;
  missing = scene.mesh_assets[scene.definitions.part_4.mesh].url;
  const absent = await newPage();
  await idle(absent);
  assert.equal(await absent.evaluate(() => cadGpuStats.ready), true);
  await focus(absent, 4);
  await idle(absent);
  await pick(absent, 'rotated_4', true);
  await focus(absent, 5);
  await idle(absent);
  await pick(absent, 'rotated_5');
  assert.ok(await absent.evaluate(() => Object.keys(cadGpuStats.assetErrors).length >= 1));
  missing = null;
  oversized = scene.mesh_assets[scene.definitions.part_2.mesh].url;
  const overflow = await newPage();
  await idle(overflow);
  await until(() => oversizedClosed >= 1, 'oversized response reader cancellation');
  const overflowErrors = await overflow.evaluate(() => Object.values(cadGpuStats.assetErrors).join(' '));
  assert.match(overflowErrors, /exceeds declared byte length/);
  await focus(overflow, 2);
  await pick(overflow, 'cell_2', true);
  oversized = null;
  const coldStart = requests.length;
  const cold = await newPage(false, '#camera=' + encodeURIComponent(JSON.stringify({ yaw: -Math.PI / 2, pitch: Math.PI / 2, target: [510, 5, 2], halfHeight: 15 })));
  await idle(cold);
  const coldRequests = requests.slice(coldStart).filter(name => assets.some(([, asset]) => asset.url === name));
  assert.deepEqual(coldRequests, [scene.mesh_assets[scene.definitions.part_5.mesh].url], 'Cold focused camera must fetch only its visible mesh');
  const coldCulling = await cullingOracle(cold);
  assert.equal(coldCulling.actual.length, 2);
  await pick(cold, 'cell_5');
  await pick(cold, 'rotated_5');
  assert.ok(peak <= 4, `More than four HTTP mesh responses in flight: ${peak}`);
  assert.equal(requests.some(name => /\.(brep\.json|step|stl)$/.test(name)), false);
  assert.deepEqual(errors, []);
  assert.deepEqual(failures, []);
  console.log(JSON.stringify({ delayed_reordered_fetches: true, rapid_camera_a_b_a: true, proxy_no_face_claim: true, captured_source: true, peak_fetches: peak, requests: requests.length, analytic_culling: true, boundary_touch: true, residency_budget: budget, eviction_refetch: true, selected_proxy_refines: true, corrupt_asset_rejected: true, missing_asset_proxy: true, cold_visible_assets: coldRequests.length, oversized_response_cancelled: true }));
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (browser) await browser.close();
  if (server) await new Promise(resolve => server.close(resolve));
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
