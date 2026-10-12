// Published native-property evidence is inspectable without querying geometry.
// Real builds cover full/selected publication and source-only live refresh.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const http = require('node:http');
const { execFileSync } = require('node:child_process');
let temporary, server, browser;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, description) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (await fn()) return;
    await delay(50);
  }
  throw Error(`Timed out: ${description}`);
}
(async () => {
  const root = path.resolve(__dirname, '../..');
  const binary = process.env.CAD_BIN || path.join(root, 'target/debug/cad');
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-property-checks-'));
  const sourcePath = path.join(temporary, 'module.star');
  const source = await fs.readFile(path.join(root, 'examples/mounted_drive_module.star'), 'utf8');
  async function build(name, text, selected = false, spacing = 100) {
    await fs.writeFile(sourcePath, text);
    const directory = path.join(temporary, name);
    execFileSync(binary, ['build', sourcePath, '--param', `module_spacing=${spacing}`,
      '--out-dir', directory, ...(selected ? ['--output', 'housing'] : [])], { stdio: 'pipe' });
    const manifest = JSON.parse(await fs.readFile(path.join(directory, 'manifest.json'), 'utf8'));
    return { name, directory, manifest, source: text };
  }
  const full = await build('full', source);
  const selected = await build('selected', source, true);
  const changedLimit = await build('limit', source.replace('max = 0.65)', 'max = 0.66)'), true);
  const changedAssembly = await build('assembly', changedLimit.source, true, 120);
  const absent = await build('absent', source.split('\n').filter(line => !line.startsWith('assert_property(')).join('\n'));
  assert.equal(selected.manifest.outputs.housing.content_hash, changedLimit.manifest.outputs.housing.content_hash);
  assert.equal(selected.manifest.outputs.housing.content_hash, changedAssembly.manifest.outputs.housing.content_hash);
  assert.notEqual(selected.manifest.recipe_context_hash, changedLimit.manifest.recipe_context_hash);
  assert.notEqual(changedLimit.manifest.recipe_context_hash, changedAssembly.manifest.recipe_context_hash);
  assert.equal(selected.manifest.outputs.mounted_drive_module, undefined, 'assertion-only target was published');
  assert.equal(full.manifest.property_assertions.mass_budget.status, 'passed');
  // Independently derived component mass, avoiding a viewer/manifest-only oracle.
  assert.ok(Math.abs(full.manifest.property_assertions.mass_budget.actual - (0.2288736 + 0.132242 * Math.PI)) < 1e-12);

  let refreshManifest = selected, refreshArtifact = selected;
  server = http.createServer(async (req, res) => {
    try {
      let pathname = new URL(req.url, 'http://localhost').pathname;
      if (pathname === '/favicon.ico') { res.writeHead(204); res.end(); return; }
      let filename;
      if (pathname.startsWith('/refresh/')) {
        filename = path.join(pathname.endsWith('/manifest.json') ? refreshManifest.directory : refreshArtifact.directory, path.basename(pathname));
      } else filename = path.join(temporary, pathname);
      res.setHeader('Content-Type', pathname.endsWith('.html') ? 'text/html' : 'application/json');
      res.end(await fs.readFile(filename));
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const base = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined,
    args: ['--no-sandbox', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [], forbidden = [], results = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (request.method() !== 'GET' || /\.(brep\.json|step|stl)(\?|$)|\/(query|queries|rpc)(\/|\?|$)/.test(request.url())) forbidden.push(request.url());
  });
  async function inspect(generation, output, mode) {
    await page.goto(`${base}/${generation.name}/${output}.${mode}.html`);
    if (mode !== 'preview') {
      await page.waitForFunction(() => window.cadGpuTest);
      await page.evaluate(async () => { await cadGpuTest.whenIdle(); cadGpuTest.draw(); cadGpuTest.finish(); });
    }
    const reports = generation.manifest.property_assertions;
    assert.equal(await page.locator('#engineering-checks [data-assertion-id]').count(), Object.keys(reports).length);
    for (const [id, report] of Object.entries(reports)) {
      const card = page.locator('#engineering-checks [data-assertion-id]').filter({ has: page.locator(`summary`, { hasText: id }) });
      assert.equal(await card.count(), 1, `missing assertion ${id}`);
      const text = await card.textContent();
      for (const value of [id, report.output, report.property.replace(/_(kg_mm2|mm5|mm3|kg|mm)$/, '').replaceAll('_', ' '), report.target_content_hash, report.actual,
        ...(report.min === null ? [] : [report.min]), ...(report.max === null ? [] : [report.max])]) {
        assert.ok(text.includes(String(value)), `Missing ${value} in ${id}: ${text}`);
      }
      assert.match(text, /passed/i);
      assert.match(text, output === report.output ? /displayed output/i : /other build output/i);
    }
    const summary = await page.locator('#engineering-checks-summary').textContent();
    assert.match(summary, Object.keys(reports).length ? /captured|build/i : /no .*checks|no .*assertions/i);
    await page.locator('#recipe-toggle').click();
    assert.equal(await page.locator('#recipe-source').textContent(), generation.source);
    results.push({ generation: generation.name, output, mode, reports: Object.keys(reports).length });
  }
  for (const mode of ['preview', 'gpu', 'stream']) {
    await inspect(full, 'mounted_drive_module', mode);
    // Hostile captured strings remain text. Corrupted/stale evidence must never
    // receive a Passed label, including the displayed output's hash mismatch.
    const hostile = await page.evaluate(() => {
      const payload = '<img src=x onerror="window.checkInjection=true">';
      const context = structuredClone(recipeContext);
      const good = context.property_assertions.mass_budget;
      context.property_assertions = {
        [payload]: { ...good, output: payload, message: payload },
        stale: { ...good, output: context.output_name, target_content_hash: 'stale' },
        unknown: { ...good, property: 'unknown_property' },
      };
      renderEngineeringChecks(context);
      const cards = [...document.querySelectorAll('#engineering-checks [data-assertion-id]')];
      return {
        literal: cards[0].textContent.includes(payload),
        id: cards[0].dataset.assertionId,
        output: cards[0].dataset.output,
        elementCount: document.querySelectorAll('#engineering-checks img, #engineering-checks script').length,
        injected: !!window.checkInjection,
        rejected: cards.slice(1).every(card => card.dataset.status === 'unavailable' && !card.textContent.includes('Passed')),
      };
    });
    assert.equal(hostile.literal, true);
    assert.equal(hostile.id, '<img src=x onerror="window.checkInjection=true">');
    assert.equal(hostile.output, hostile.id);
    assert.equal(hostile.elementCount, 0);
    assert.equal(hostile.injected, false);
    assert.equal(hostile.rejected, true);
  }
  await inspect(selected, 'housing', 'preview');
  for (const mode of ['preview', 'gpu', 'stream']) await inspect(absent, 'mounted_drive_module', mode);

  // Bounds-only edits preserve output geometry, but must refresh the evidence.
  await page.goto(`${base}/refresh/housing.preview.html`);
  await until(async () => await page.locator('.badge').textContent() === 'LIVE PREVIEW', 'initial refresh baseline');
  const camera = await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom }));
  refreshManifest = changedLimit;
  await until(async () => await page.locator('.badge').textContent() === 'RECONNECTING', 'reject older artifact against changed context');
  assert.match(await page.locator('#engineering-checks').textContent(), /0\.65/);
  refreshArtifact = changedLimit;
  await until(async () => (await page.locator('#engineering-checks').textContent()).includes('0.66'), 'bounds-only evidence refresh');
  assert.equal(await page.locator('.badge').textContent(), 'LIVE PREVIEW');
  assert.equal(await page.locator('#recipe-source').textContent(), changedLimit.source);

  // Selected body remains identical while an unpublished assembly's measured
  // balance and target hash change. Global evidence must follow that build.
  refreshManifest = changedAssembly; refreshArtifact = changedAssembly;
  const balance = changedAssembly.manifest.property_assertions.balance_envelope;
  await until(async () => (await page.locator('#engineering-checks').textContent()).includes(String(balance.actual)), 'global target evidence refresh');
  const refreshed = await page.locator('#engineering-checks').textContent();
  assert.ok(refreshed.includes(balance.target_content_hash));
  assert.equal(await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom })), camera);
  await page.locator('#engineering-checks [data-assertion-id]').evaluateAll(elements => elements.forEach(element => { element.open = true; }));
  await page.locator('#engineering-checks').scrollIntoViewIfNeeded();
  await page.screenshot({ path: '/tmp/cad-engineering-checks.png' });

  assert.deepEqual(errors, []);
  assert.deepEqual(forbidden, []);
  console.log(JSON.stringify({ results, boundsOnlyRefresh: true, globalTargetRefresh: true,
    mismatchedGenerationRejected: true, textOnlyEvidence: true, malformedAndStaleEvidenceRejected: true, exactGeometryAndQueryRequests: 0 }, null, 2));
})().catch(error => { console.error(error); process.exitCode = 1; }).finally(async () => {
  if (browser) await browser.close();
  if (server) await new Promise(resolve => server.close(resolve));
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
