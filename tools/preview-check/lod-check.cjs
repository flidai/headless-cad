// Real analytic ring geometry, controlled transport, and independent pick points.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const http = require('node:http');
const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
let temporary, server, browser;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, label) {
  const end = Date.now() + 15000;
  while (Date.now() < end) { if (await fn()) return; await sleep(30); }
  throw Error(`Timed out: ${label}`);
}
(async () => {
  const root = path.resolve(__dirname, '../..');
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-lod-'));
  const source = `# Repeated annular parts retain their bore and face identity at every LOD.
body=extrude(id='ring',profile=circle(id='profile',radius=16,inner_radius=10),height=6)
p=part(id='ring',body=body,material='Steel',density_kg_per_m3=7800)
assembly_output(name='rings',assembly=assembly(id='rings',instances=[instance(id='ring_'+str(i),part=p,offset=[60*i,0,0]) for i in range(3)]))
assert_property(id='volume',output='rings',property='volume_mm3',min=8821,max=8822)
`;
  const recipe = path.join(temporary, 'rings.star'), output = path.join(temporary, 'out');
  await fs.writeFile(recipe, source);
  execFileSync(process.env.CAD_BIN || path.join(root, 'target/debug/cad'), ['build', recipe, '--out-dir', output], { stdio: 'pipe' });
  const scene = JSON.parse(await fs.readFile(path.join(output, 'rings.stream.json'), 'utf8'));
  const hash = scene.definitions.ring.mesh, levels = scene.mesh_lods[hash];
  assert.equal(levels.length, 3);
  assert.equal(scene.instances.length, 3);
  assert.equal(Object.keys(scene.mesh_lods).length, 1);
  for (const [i, descriptor] of levels.entries()) {
    const bytes = await fs.readFile(path.join(output, descriptor.url));
    assert.equal(crypto.createHash('sha256').update(bytes).digest('hex'), descriptor.sha256);
    if (i) { assert.ok(descriptor.geometric_error_mm < levels[i-1].geometric_error_mm); assert.ok(descriptor.gpu_bytes > levels[i-1].gpu_bytes); }
  }
  const coarse = levels[0], fine = levels[2];
  let gateFine = false, failFine = false, forgeErrors = false, forgePlacement = false;
  const pending = new Set(), requests = [], errors = [], forbidden = [];
  server = http.createServer(async (req, res) => {
    const name = path.basename(new URL(req.url, 'http://localhost').pathname);
    requests.push(name);
    if (name === 'favicon.ico') { res.writeHead(204); res.end(); return; }
    try {
      if (gateFine && name === fine.url) await new Promise(resolve => {
        pending.add(resolve); res.once('close', () => { pending.delete(resolve); resolve(); });
      });
      if (res.destroyed) return;
      if (failFine && name === fine.url) { res.writeHead(404); res.end(); return; }
      res.setHeader('Content-Type', name.endsWith('.html') ? 'text/html' : 'application/json');
      let bytes = await fs.readFile(path.join(output, name));
      if ((forgeErrors || forgePlacement) && name.endsWith('.html')) {
        bytes = bytes.toString().replace(/(<script id="cad-scene-data(?:-v[23])?" type="application\/json">)([^\r\n]*)(<\/script>)/, (_, start, data, end) => {
          const tampered = JSON.parse(data);
          if (forgeErrors) {
            for (const entry of Object.values(tampered.mesh_assets)) entry.geometric_error_mm = 0;
            for (const list of Object.values(tampered.mesh_lods)) for (const entry of list) entry.geometric_error_mm = 0;
          }
          if (forgePlacement) tampered.instances[0].transform.rotation = [0,0,2,2];
          return start + JSON.stringify(tampered) + end;
        });
      }
      res.end(bytes);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined,
    args: ['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader'] });
  const wide = { yaw: -Math.PI/2, pitch: Math.PI/2, target: [0,0,3], halfHeight: 300 };
  const base = `http://127.0.0.1:${server.address().port}/rings.stream.html#camera=${encodeURIComponent(JSON.stringify(wide))}`;
  async function page(ignoreAbort = false) {
    const p = await browser.newPage({ viewport: { width: 1280, height: 900 } });
    if (ignoreAbort) await p.addInitScript(() => {
      const original = window.fetch.bind(window);
      window.fetch = (input, options) => original(input, String(input).endsWith('.display.json') ? { ...options, signal: undefined } : options);
    });
    p.on('pageerror', e => errors.push(e.message));
    p.on('request', r => { if (/\.(brep\.json|step|stl)(\?|$)|\/(query|rpc)(\/|$)/.test(r.url())) forbidden.push(r.url()); });
    await p.goto(base); await p.waitForFunction(() => window.cadGpuTest && cadGpuStats.ready); await idle(p); return p;
  }
  async function idle(p) {
    await p.evaluate(() => Promise.race([cadGpuTest.whenIdle(), new Promise((_, reject) => setTimeout(() => reject(Error('LOD scheduler did not settle')), 10000))]));
    await p.evaluate(() => { cadGpuTest.draw(); cadGpuTest.finish(); });
  }
  async function camera(p, height) { await p.evaluate(height => { cadGpuTest.setCamera({ halfHeight: height }); cadGpuTest.draw(); }, height); }
  const stats = p => p.evaluate(() => structuredClone(cadGpuStats));
  async function pickCapAndBore(p) {
    const hits = await p.evaluate(() => {
      const pick = point => { const [x,y] = cadGpuTest.project(point); return cadGpuTest.pick(x,y); };
      return [pick([13,1,6]), pick([0,0,6])];
    });
    // Authored annulus has top native face1; its central hole contains no surface.
    assert.equal(hits[0]?.instance, 0); assert.equal(hits[0]?.face, 1);
    assert.ok(hits[1] === null || hits[1].instance < 0, JSON.stringify(hits));
  }
  const p = await page();
  let s = await stats(p);
  assert.equal(s.activeLodLevels[hash], 0);
  assert.equal(s.residentMeshCount, 1, 'repeated occurrences share one resident coarse mesh');
  assert.equal(s.unmetDetailCount, 0);
  assert.ok(s.maxScreenErrorPixels <= 1);
  assert.equal(requests.filter(name => name === coarse.url).length, 1);
  assert.equal(requests.filter(name => name === fine.url).length, 0);
  const coarseBytes = coarse.byte_length + coarse.gpu_bytes;
  assert.ok(coarseBytes < fine.byte_length + fine.gpu_bytes);

  gateFine = true;
  await camera(p, 25);
  await until(() => pending.size > 0, 'fine refinement requested');
  s = await stats(p);
  assert.equal(s.activeLodLevels[hash], 0, 'coarse fallback survives pending refinement');
  assert.equal(s.proxyCount, 0);
  assert.ok(s.unmetDetailCount > 0);
  await pickCapAndBore(p);
  gateFine = false; for (const resolve of pending) resolve(); pending.clear();
  await idle(p);
  s = await stats(p);
  assert.equal(s.activeLodLevels[hash], 2);
  assert.equal(s.unmetDetailCount, 0);
  assert.ok(s.maxScreenErrorPixels <= 1);
  await pickCapAndBore(p);
  await p.locator('#recipe-toggle').click();
  assert.equal(await p.locator('#recipe-source').textContent(), source);
  assert.match(await p.locator('#engineering-checks').textContent(), /Passed/);
  await camera(p, .2); await idle(p);
  s = await stats(p);
  assert.equal(s.activeLodLevels[hash], 2);
  assert.ok(s.unmetDetailCount > 0, 'finest available geometry cannot satisfy infinite zoom');
  assert.match(await p.locator('#stream-status').textContent(), /detail|pixel|target/i);
  await camera(p, 300); await idle(p);
  assert.equal((await stats(p)).activeLodLevels[hash], 0);
  await p.close();

  const bounded = await page();
  await bounded.evaluate(bytes => cadGpuTest.setBudget(bytes), coarseBytes);
  await camera(bounded, 25); await idle(bounded);
  s = await stats(bounded);
  assert.equal(s.activeLodLevels[hash], 0);
  assert.equal(s.proxyCount, 0);
  assert.ok(s.unmetDetailCount > 0);
  assert.ok(s.residentBytes + s.reservedBytes <= coarseBytes);
  await pickCapAndBore(bounded);
  await bounded.close();

  const intermediate = await page();
  const mediumBudget = coarseBytes + levels[1].byte_length + levels[1].gpu_bytes;
  await intermediate.evaluate(bytes => cadGpuTest.setBudget(bytes), mediumBudget);
  await camera(intermediate, 25); await idle(intermediate);
  s = await stats(intermediate);
  assert.equal(s.activeLodLevels[hash], 1, 'use an affordable intermediate when finest cannot fit');
  assert.equal(s.proxyCount, 0); assert.ok(s.unmetDetailCount > 0);
  assert.ok(s.residentBytes + s.reservedBytes <= mediumBudget);
  await pickCapAndBore(intermediate); await intermediate.close();

  const racing = await page(true);
  gateFine = true; await camera(racing, 25);
  await until(() => pending.size > 0, 'gated refinement before camera reversal');
  await camera(racing, 300);
  gateFine = false; for (const resolve of pending) resolve(); pending.clear();
  await idle(racing); s = await stats(racing);
  assert.equal(s.activeLodLevels[hash], 0, 'obsolete fine completion cannot replace current coarse level');
  assert.ok(s.abortedRequests > 0);
  assert.equal(s.geometryUploads, 1);
  await camera(racing, 25); await idle(racing);
  assert.equal((await stats(racing)).activeLodLevels[hash], 2);
  await racing.close();

  const missing = await page();
  failFine = true; await camera(missing, 25); await idle(missing);
  s = await stats(missing);
  assert.ok(s.activeLodLevels[hash] < 2);
  assert.equal(s.proxyCount, 0);
  assert.ok(s.unmetDetailCount > 0);
  assert.ok(Object.keys(s.assetErrors).length > 0);
  await pickCapAndBore(missing);
  await missing.close(); failFine = false;

  forgeErrors = true;
  const forged = await page(); s = await stats(forged);
  assert.equal(s.residentMeshCount, 0, 'unverified error metadata cannot claim a precise resident surface');
  assert.ok(Object.keys(s.assetErrors).length > 0);
  assert.ok(s.unmetDetailCount > 0);
  await forged.close(); forgeErrors = false;
  forgePlacement = true;
  const invalid = await browser.newPage(); await invalid.goto(base);
  await invalid.waitForFunction(() => window.cadGpuStats?.errors.length > 0);
  assert.match(await invalid.locator('#error').textContent(), /rigid display placement/);
  assert.equal(await invalid.evaluate(() => cadGpuStats.ready), false);
  await invalid.close(); forgePlacement = false;
  assert.deepEqual(errors, []); assert.deepEqual(forbidden, []);
  console.log(JSON.stringify({ sharedOccurrences: 3, levels: levels.map(d => ({ errorMm: d.geometric_error_mm, bytes: d.byte_length, gpuBytes: d.gpu_bytes })),
    delayedRefinementKeepsSurface: true, budgetFallbackSettles: true, missingDetailKeepsSurface: true,
    intermediateBudgetRefinement: true, obsoleteCompletionRejected: true, forgedErrorRejected: true,
    capAndBoreIdentityPreserved: true, finestLimitReported: true, exactLoads: 0 }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (browser) await browser.close();
  if (server) await new Promise(resolve => server.close(resolve));
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
