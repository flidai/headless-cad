// Deterministic live-refresh race regression: a complete HTML artifact from one
// atomic build must never be installed under another generation's manifest.
const { chromium } = require('playwright');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');

const execute = promisify(execFile);
const binary = process.env.CAD_BIN || path.resolve(__dirname, '../../target/debug/cad');
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
let browser, temporary;

async function until(fn, description, timeout = 15000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { if (await fn()) return; } catch (error) { last = error; }
    await delay(50);
  }
  throw new Error(`Timed out: ${description}${last ? ` (${last.message})` : ''}`);
}

function recipe(width) {
  return `profile = rectangle(id = "profile", width = ${width}, depth = 4)\npart = extrude(id = "part", profile = profile, height = 2)\noutput(name = "part", feature = part)\n`;
}

(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-refresh-check-'));
  const source = path.join(temporary, 'main.star');
  async function generation(width, name) {
    const output = path.join(temporary, name);
    const text = recipe(width);
    await fs.writeFile(source, text);
    await execute(binary, ['build', source, '--out-dir', output], { maxBuffer: 4 * 1024 * 1024 });
    return {
      source: text,
      html: await fs.readFile(path.join(output, 'part.preview.html'), 'utf8'),
      manifest: await fs.readFile(path.join(output, 'manifest.json'), 'utf8'),
      volume: width * 4 * 2,
    };
  }
  const a = await generation(10, 'a');
  const b = await generation(20, 'b');
  browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CAD_BROWSER || undefined,
    args: ['--no-sandbox'],
  });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  let manifest = a;
  let artifact = b;
  let previewFetches = 0;
  await page.route('http://cad-refresh.test/**', async route => {
    const request = route.request();
    if (request.url().endsWith('/manifest.json')) {
      return route.fulfill({ status: 200, contentType: 'application/json', body: manifest.manifest });
    }
    if (request.url().endsWith('/part.preview.html')) {
      const navigation = request.isNavigationRequest();
      if (!navigation) previewFetches += 1;
      return route.fulfill({ status: 200, contentType: 'text/html', body: navigation ? a.html : artifact.html });
    }
    return route.fulfill({ status: 404, body: 'Not found' });
  });

  // Simulate A -> B -> A publication between the manifest/HTML requests:
  // the current manifest is A, but a fetched, fully valid HTML response is B.
  await page.goto('http://cad-refresh.test/part.preview.html');
  await page.getByRole('button', { name: 'Recipe', exact: true }).click();
  await page.getByRole('button', { name: 'Top', exact: true }).click();
  const camera = await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom, drawEdges }));
  await until(async () => previewFetches > 0 && await page.evaluate(() =>
    document.querySelector('.badge').textContent === 'RECONNECTING' || scene.volume !== 80
  ), 'mismatched generation is processed');
  assert.equal(await page.evaluate(() => scene.volume), a.volume, 'installed geometry from a mismatched generation');
  assert.equal(await page.locator('#recipe-source').textContent(), a.source, 'installed source from a mismatched generation');
  assert.equal(await page.locator('.badge').textContent(), 'RECONNECTING');

  // The rejected response must not advance the baseline or prevent recovery.
  manifest = b;
  artifact = b;
  await until(async () => await page.evaluate(() => scene.volume) === b.volume
    && await page.locator('#recipe-source').textContent() === b.source, 'correct generation B recovers');
  assert.equal(await page.locator('.badge').textContent(), 'LIVE PREVIEW');

  // Reverting the edit legitimately restores the earlier content/context IDs.
  manifest = a;
  artifact = a;
  await until(async () => await page.evaluate(() => scene.volume) === a.volume
    && await page.locator('#recipe-source').textContent() === a.source, 'edit revert restores generation A');
  assert.equal(await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom, drawEdges })), camera);
  assert.equal(await page.getByRole('button', { name: 'Recipe', exact: true }).getAttribute('aria-expanded'), 'true');
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({
    rejected_mismatched_generation: true,
    retained_matching_source_and_geometry: true,
    recovery: true,
    edit_revert: true,
    camera_and_recipe_panel_preserved: true,
    errors,
  }));
})().catch(error => {
  console.error(error);
  process.exitCode = 1;
}).finally(async () => {
  if (browser) await browser.close();
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
