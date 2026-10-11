const { chromium } = require('playwright');
const { spawn } = require('node:child_process');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const assert = require('node:assert/strict');

const binary = process.env.CAD_BIN || path.resolve(__dirname, '../../target/debug/cad');
const children = [];
let browser, temporary;
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(fn, description, timeout = 20000) {
  const deadline = Date.now() + timeout;
  let last;
  while (Date.now() < deadline) {
    try { const result = await fn(); if (result) return result; } catch (error) { last = error; }
    await delay(100);
  }
  throw new Error(`Timed out: ${description}${last ? ` (${last.message})` : ''}`);
}
function launch(args) {
  const child = spawn(binary, args, { stdio: ['ignore', 'pipe', 'pipe'] });
  child.log = '';
  child.stdout.on('data', data => child.log += data);
  child.stderr.on('data', data => child.log += data);
  children.push(child);
  return child;
}
function recipe(width) {
  return `width = param(name = "width", default = ${width})\ndepth = param(name = "depth", default = 30)\nprofile = rectangle(id = "profile", width = width, depth = depth)\npart = extrude(id = "part", profile = profile, height = 6)\noutput(name = "plate", feature = part)\n`;
}
(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-hosted-check-'));
  const source = path.join(temporary, 'main.star');
  const output = path.join(temporary, 'out');
  await fs.writeFile(source, recipe(80));
  const watcher = launch(['watch', source, '--out-dir', output, '--interval-ms', '100', '--param', 'depth=40']);
  const manifest = async () => JSON.parse(await fs.readFile(path.join(output, 'manifest.json'), 'utf8'));
  await until(async () => (await manifest()).outputs.plate.volume_mm3 === 19200, 'initial build');
  const server = launch(['serve', output, '--port', '0']);
  const base = await until(() => {
    const match = server.log.match(/http:\/\/(?:127\.0\.0\.1|localhost):\d+/);
    return match && match[0];
  }, 'server startup URL');
  browser = await chromium.launch({ headless: true, executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => {
    if (request.method() !== 'GET') errors.push(`Unexpected write request: ${request.method()}`);
  });
  await page.goto(base);
  await page.locator('a[href$="plate.preview.html"]').first().click();
  await page.waitForSelector('canvas');
  await page.getByRole('button', { name: 'Recipe', exact: true }).click();
  assert.equal(await page.locator('#recipe-source').textContent(), recipe(80));
  assert.match(await page.locator('#recipe-parameters').textContent(), /depth[\s\S]*40/);
  await page.getByRole('button', { name: 'Top', exact: true }).click();
  await page.getByRole('button', { name: 'Edges', exact: true }).click();
  const camera = await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom, drawEdges }));
  const initial = await page.evaluate(() => JSON.stringify(scene));
  // Wait for the preview's first manifest poll before editing the recipe.
  await page.waitForTimeout(2500);
  await fs.writeFile(source, recipe(100));
  await until(async () => (await manifest()).outputs.plate.volume_mm3 === 24000, 'rebuild');
  await until(async () => (await page.evaluate(() => JSON.stringify(scene))) !== initial, 'automatic preview refresh');
  assert.equal(await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom, drawEdges })), camera, 'refresh changed inspection view');
  const successful = await page.evaluate(() => JSON.stringify(scene));
  const artifact = await fs.readFile(path.join(output, 'plate.brep.json'), 'utf8');
  await fs.writeFile(source, 'this is invalid starlark !!!');
  await until(() => /fail|error/i.test(watcher.log), 'failed build diagnostic');
  assert.equal(await fs.readFile(path.join(output, 'plate.brep.json'), 'utf8'), artifact, 'failure replaced last valid geometry');
  assert.equal(await page.evaluate(() => JSON.stringify(scene)), successful, 'failed rebuild changed viewer');
  await fs.writeFile(source, recipe(120));
  await until(async () => (await manifest()).outputs.plate.volume_mm3 === 28800, 'recovery build');
  await until(async () => (await page.evaluate(() => JSON.stringify(scene))) !== successful, 'recovery preview refresh');
  await until(async () => (await page.locator('#recipe-source').textContent()) === recipe(120), 'source matches recovered model');
  const beforeComment = await manifest();
  const hostileComment = '# </script><img src=x onerror="window.__sourceXss=1">\n';
  const annotated = recipe(120) + hostileComment;
  await fs.writeFile(source, annotated);
  await until(async () => (await manifest()).recipe_source_hash !== beforeComment.recipe_source_hash, 'comment-only source rebuild');
  assert.equal((await manifest()).outputs.plate.content_hash, beforeComment.outputs.plate.content_hash, 'comment changed geometry hash');
  await until(async () => (await page.locator('#recipe-source').textContent()) === annotated, 'comment-only source refresh');
  assert.equal(await page.evaluate(() => window.__sourceXss), undefined);
  assert.equal(await page.locator('img[src="x"]').count(), 0);
  assert.equal(await page.evaluate(() => JSON.stringify({ yaw, pitch, zoom, drawEdges })), camera);
  const sourceResponse = await fetch(`${base}/recipe.star`);
  assert.equal(sourceResponse.status, 200);
  assert.equal(await sourceResponse.text(), annotated);
  // Valid geometry with invalid design intent must also retain the complete
  // previous generation. Restore an assertion that resolves without changing
  // geometry and verify the captured source refreshes independently.
  const previousManifest = await fs.readFile(path.join(output, 'manifest.json'), 'utf8');
  const previousScene = await page.evaluate(() => JSON.stringify(scene));
  const logStart = watcher.log.length;
  const assertion = count => `assert_faces(id="live_top_intent", from_feature="part", role="end_cap", in_feature="part", count=${count})\n`;
  await fs.writeFile(source, annotated + assertion(2));
  await until(() => /live_top_intent/.test(watcher.log.slice(logStart)), 'failed face assertion');
  assert.equal(await fs.readFile(path.join(output, 'manifest.json'), 'utf8'), previousManifest);
  assert.equal(await page.evaluate(() => JSON.stringify(scene)), previousScene);
  const assertedSource = annotated + assertion(1);
  await fs.writeFile(source, assertedSource);
  await until(async () => (await manifest()).face_assertions?.live_top_intent?.status === 'resolved', 'assertion recovery');
  await until(async () => (await page.locator('#recipe-source').textContent()) === assertedSource, 'assertion source refresh');
  assert.equal(await page.evaluate(() => JSON.stringify(scene)), previousScene);
  const response = await fetch(`${base}/plate.step`);
  assert.equal(response.status, 200);
  assert.match(await response.text(), /ISO-10303-21/);
  assert.deepEqual(errors, []);
  console.log(JSON.stringify({ hosted_viewer: true, live_recipe_rebuild: true, automatic_refresh: true, camera_preserved: true, failed_build_retained: true, failed_assertion_retained: true, assertion_recovery: true, recovery: true, step_download: true, exact_recipe_source: true, source_only_refresh: true, source_html_escaped: true, errors }));
})().catch(error => {
  console.error(error);
  for (const child of children) console.error(child.log);
  process.exitCode = 1;
}).finally(async () => {
  if (browser) await browser.close();
  for (const child of children) child.kill('SIGTERM');
  await Promise.all(children.map(child => child.exitCode !== null || child.signalCode !== null ? Promise.resolve() : new Promise(resolve => child.once('exit', resolve))));
  if (temporary) await fs.rm(temporary, { recursive: true, force: true });
});
