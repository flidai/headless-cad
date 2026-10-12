// Reproducible coarse-overview -> station-detail workload, not a hardware gate.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const os = require('node:os');
const { execFileSync } = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
(async () => {
  const url = process.env.CAD_LOD_URL || 'http://localhost:8080/spindle_scale/spindle_line.stream.html';
  const software = process.env.CAD_GPU_HARDWARE !== '1';
  const args = ['--no-sandbox'];
  if (software) args.push('--use-angle=swiftshader', '--enable-unsafe-swiftshader');
  const browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
    const requests = [], errors = [];
    page.on('request', request => requests.push(request.url()));
    page.on('pageerror', error => errors.push(error.message));
    async function settled() {
      await page.evaluate(() => Promise.race([cadGpuTest.whenIdle(), new Promise((_, reject) => setTimeout(() => reject(Error('Detail did not settle')), 30000))]));
      await page.evaluate(() => { cadGpuTest.draw(); cadGpuTest.finish(); });
    }
    async function snapshot() {
      return page.evaluate(() => {
        const stats = cadGpuStats, gl = document.querySelector('canvas').getContext('webgl2');
        const ext = gl.getExtension('WEBGL_debug_renderer_info');
        const camera = cadGpuTest.snapshot();
        delete camera.visibleIndices; delete camera.residentKeys;
        return { renderer: ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
          instances: stats.instanceCount, visible: stats.visibleCount, uniqueParts: stats.meshCount,
          residentMeshes: stats.residentMeshCount, residentBytes: stats.residentBytes, reservedBytes: stats.reservedBytes,
          activeLevels: stats.activeLodLevels, maxScreenErrorPixels: stats.maxScreenErrorPixels,
          unmetDetailCount: stats.unmetDetailCount, proxies: stats.proxyCount, errors: stats.errors,
          geometryVertices: stats.geometryVertices, drawCalls: stats.drawCalls, requested: stats.requested,
          camera, assemblyHash: compactScene.content_hash, contextHash: recipeContext.recipe_context_hash,
          parameters: recipeContext.parameters,
          finestEncodedBytes: Object.values(compactScene.mesh_assets).reduce((sum,d) => sum + d.byte_length, 0),
          finestGeometryBytes: Object.values(compactScene.mesh_assets).reduce((sum,d) => sum + d.gpu_bytes, 0) };
      });
    }
    const start = performance.now();
    const response = await page.goto(url);
    assert.ok(response?.ok());
    const previewSha256 = crypto.createHash('sha256').update(await response.body()).digest('hex');
    await page.waitForFunction(() => window.cadGpuTest && cadGpuStats.ready);
    const proxyReadyMs = performance.now() - start;
    await settled(); const coarseReadyMs = performance.now() - start;
    const overview = await snapshot();
    assert.ok(overview.instances >= 4000, 'use the 1000-station fixture');
    assert.equal(overview.uniqueParts, 15);
    assert.ok(Object.values(overview.activeLevels).every(level => level === 0));
    assert.equal(overview.proxies, 0); assert.equal(overview.unmetDetailCount, 0);
    assert.ok(overview.residentBytes < overview.finestEncodedBytes + overview.finestGeometryBytes);
    await page.screenshot({ path: '/tmp/cad-spindle-lod-overview.png' });
    const focusStart = performance.now();
    await page.evaluate(() => { cadGpuTest.setView('top'); cadGpuTest.setCamera({ target: [0,0,30], halfHeight: 30 }); cadGpuTest.draw(); });
    await settled(); const detailReadyMs = performance.now() - focusStart;
    const detail = await snapshot();
    assert.equal(detail.visible, 4); assert.equal(detail.proxies, 0);
    assert.equal(detail.unmetDetailCount, 0);
    assert.ok(Object.values(detail.activeLevels).every(level => level === 2));
    const picked = await page.evaluate(() => { const [x,y] = cadGpuTest.project([12,0,45]); return cadGpuTest.pick(x,y); });
    assert.equal(picked.face, 1, 'annular retainer top keeps its native face identity');
    assert.equal(await page.evaluate(index => compactScene.instance_paths[compactScene.instances[index].id].at(-1), picked.instance), 'retainer_0');
    await page.screenshot({ path: '/tmp/cad-spindle-lod-detail.png' });
    assert.deepEqual(errors, []); assert.deepEqual(overview.errors, []); assert.deepEqual(detail.errors, []);
    assert.equal(requests.filter(url => /\.(brep\.json|step|stl)(\?|$)/.test(url)).length, 0);
    console.log(JSON.stringify({ schema_version: 1, revision: execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),
      working_diff_sha256: crypto.createHash('sha256').update(execFileSync('git',['diff','HEAD'])).digest('hex'),
      benchmark_sha256: crypto.createHash('sha256').update(fs.readFileSync(__filename)).digest('hex'),
      preview_sha256: previewSha256,
      date_utc: new Date().toISOString(), url, browser: browser.version(), software_renderer_requested: software,
      environment: { os: os.type(), release: os.release(), cpu: os.cpus()[0]?.model, logical_cpus: os.cpus().length },
      viewport: [1440,1000], device_scale_factor: 1, proxy_ready_ms: proxyReadyMs, coarse_ready_ms: coarseReadyMs,
      focus_detail_ready_ms: detailReadyMs, timing_scope: 'cold page to coarse coverage, then station detail including fetch and GPU completion; not presentation latency',
      overview, detail, requests, exact_geometry_requests: 0 }, null, 2));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
