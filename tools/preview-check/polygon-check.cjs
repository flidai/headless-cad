const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const { pathToFileURL } = require('node:url');

(async () => {
  const artifact = process.env.CAD_POLYGON_PREVIEW || path.resolve(__dirname, '../../out/angled_fixture/ramp_definition.preview.html');
  const browser = await chromium.launch({ executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox'] });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 2 });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(pathToFileURL(artifact).href);
    await page.getByRole('button', { name: 'Top', exact: true }).click();
    const result = await page.evaluate(() => {
      // Independent XY half-plane oracle for the exact 40x20 triangular cap.
      const cap = scene.faces.find(face => face.normal[2] === 1);
      if (!cap || cap.points.length !== 3) throw Error('Missing authoritative triangular cap');
      const scale = Math.min((width - 80) / 40, (height - 150) / 20);
      const screen = (x, y) => [width / 2 + (x - 20) * scale, height / 2 + 20 - (y - 10) * scale];
      let samples = 0, inside = 0, outside = 0;
      for (let x = -2; x <= 42; x += 2) for (let y = -2; y <= 22; y += 2) {
        if (Math.abs(x) < .5 || Math.abs(y) < .5 || Math.abs(x / 40 + y / 20 - 1) < .04) continue;
        const [sx, sy] = screen(x, y), bx = Math.floor(sx * renderRatio), by = Math.floor(sy * renderRatio);
        if (bx < 0 || by < 0 || bx >= bufferWidth || by >= bufferHeight) continue;
        const expected = x > 0 && y > 0 && x / 40 + y / 20 < 1;
        const actual = faceBuffer[by * bufferWidth + bx];
        if (actual !== (expected ? cap.id : -1)) throw Error(`Triangle silhouette mismatch at ${x},${y}`);
        samples++; if (expected) inside++; else outside++;
      }
      select(...screen(10, 5));
      if (selected !== cap.id) throw Error('Triangle picking failed');
      select(...screen(35, 15));
      if (selected !== -1) throw Error('Bounding-box corner incorrectly pickable');
      return { samples, inside, outside, triangle_picking: true, empty_corner: true };
    });
    assert.ok(result.inside > 20 && result.outside > 20);
    assert.deepEqual(errors, []);
    console.log(JSON.stringify({ ...result, errors }));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
