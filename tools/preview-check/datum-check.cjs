const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
(async () => {
  const artifact = process.env.CAD_DATUM_PREVIEW || path.resolve(__dirname, '../../out/datum_gantry/inspection_line.preview.html');
  const browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox'] });
  try {
    const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(pathToFileURL(artifact).href);
    const result = await page.evaluate(() => {
      // Analytic quarter-turn: local tool (x,y,z) -> (52+x,142+z,55-y).
      const id = JSON.stringify(['station_b', 'tool']);
      const instance = scene.instances.find(item => item.id === id);
      const faces = scene.faces.filter(face => face.instance_id === id);
      const close = (a,b) => a.length === b.length && a.every((v,i) => Math.abs(v-b[i]) < 1e-9);
      if (!instance || !close(instance.transform.translation, [52,142,55])) throw Error('Incorrect derived attachment origin');
      const expected = [[52,142,55],[68,142,55],[52,142,43],[52,150,55],[68,150,55],[52,150,43]];
      const actual = faces.flatMap(face => face.points);
      if (!actual.every(p => expected.some(q => close(p,q))) || !expected.every(p => actual.some(q => close(p,q)))) throw Error('Incorrect attached tool vertices');
      if (instance.attachment) throw Error('Display must contain resolved placements');
      return { id, faces: faces.length, occurrences: scene.instances.length, sharedMeshes: Object.keys(compactScene.meshes).length };
    });
    assert.equal(result.faces, 5);
    assert.equal(result.occurrences, 14);
    assert.equal(result.sharedMeshes, 5);
    await page.locator('#occurrence-select').selectOption(result.id);
    await page.getByRole('button', {name: 'Recipe', exact: true}).click();
    assert.match(await page.locator('#recipe-summary').textContent(), /Datum mount: local origin \[8, 0, 0\]/);
    assert.match(await page.locator('#recipe-summary').textContent(), /station_b/);
    assert.equal(await page.locator('#recipe-source').textContent(), fs.readFileSync(path.resolve(__dirname, '../../examples/datum_gantry.star'), 'utf8'));
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({...result, datum_inspection:true, captured_source:true, errors}));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
