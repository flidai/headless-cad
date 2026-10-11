const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const artifactRoot = process.env.CAD_NESTED_OUT || path.resolve(__dirname, '../../out/modular_rack');
  const expectedSource = await fs.readFile(path.resolve(__dirname, '../../examples/modular_rack.star'), 'utf8');
  const browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox'] });
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(pathToFileURL(path.join(artifactRoot, 'modular_rack.preview.html')).href);
    const id = JSON.stringify(['rear', 'carriage_b', 'guide_right']);
    await page.locator('#occurrence-select').selectOption(id);
    await page.getByRole('button', { name: 'Recipe', exact: true }).click();
    assert.equal(await page.locator('#recipe-source').textContent(), expectedSource);
    assert.match(await page.locator('#recipe-summary').textContent(), /"rear" \/ "carriage_b" \/ "guide_right"/);
    assert.match(await page.locator('#recipe-summary').textContent(), /Part number: RACK-GUIDE/);
    assert.match(await page.locator('#recipe-summary').textContent(), /Material: Demo steel/);
    const state = await page.evaluate(id => {
      const faces = scene.faces.filter(face => face.instance_id === id);
      const points = faces.flatMap(face => face.points);
      return {
        occurrences: scene.instances.length,
        parts: Object.keys(scene.definitions).length,
        assemblies: scene.assembly_definitions,
        meshes: scene.unique_meshes,
        mass_kg: scene.total_mass_kg,
        selected: selectedInstance,
        path: scene.instance_paths[id],
        min: [0, 1, 2].map(axis => Math.min(...points.map(p => p[axis]))),
        max: [0, 1, 2].map(axis => Math.max(...points.map(p => p[axis]))),
        features: [...new Set(faces.map(face => face.feature_id))],
      };
    }, id);
    assert.equal(state.occurrences, 17);
    assert.equal(state.parts, 4);
    assert.equal(state.assemblies, 3);
    assert.equal(state.meshes, 4);
    assert.ok(Math.abs(state.mass_kg - 0.2058336) < 1e-12);
    assert.equal(state.selected, id);
    assert.deepEqual(state.path, ['rear', 'carriage_b', 'guide_right']);
    assert.deepEqual(state.features, ['guide']);
    // Independent dimensions: local right guide x36..40/y0..24, carriage B
    // adds48x; the rear row rotates90° and moves to100x/32y/3z.
    for (let axis = 0; axis < 3; axis++) {
      assert.ok(Math.abs(state.min[axis] - [76, 116, 7][axis]) < 1e-8);
      assert.ok(Math.abs(state.max[axis] - [100, 120, 17][axis]) < 1e-8);
    }
    assert.equal(await page.evaluate(id => {
      const malformed = structuredClone(compactScene);
      malformed.instance_paths[id] = [3];
      try { normalizeScene(malformed); return false; } catch { return true; }
    }, id), true);
    await page.setViewportSize({ width: 390, height: 844 });
    assert.equal(await page.locator('#occurrence-select').inputValue(), id);
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({ nested_paths: true, exact_recipe_source: true, independent_world_bounds: true, shared_meshes: 4, occurrences: 17, malformed_paths_rejected: true, errors }));
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
