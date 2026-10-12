// Inspect solved planar mechanism occurrences and captured recipe modules.
// This checks the read-only viewers; native joint certification lives in the build.
const {chromium} = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const base = (process.env.CAD_BASE_URL || 'http://localhost:8080').replace(/\/$/, '');
  const name = 'solved_four_bar';
  const directory = `${base}/${name}`;
  const browser = await chromium.launch({executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const reports=[];
  try {
    const context=await browser.newContext({viewport:{width:1440,height:1000}});
    const read=async file=>{const response=await context.request.get(`${directory}/${file}`);assert.ok(response.ok(),`${file}: ${response.status()}`);return response.json();};
    const manifest=await read('manifest.json');
    const assembly=await read(`${name}.assembly.json`);
    const sourceResponse=await context.request.get(`${directory}/recipe.star`);
    assert.ok(sourceResponse.ok());
    const source=await sourceResponse.text();
    const selectedPath=['crank_unit','link'];
    const ids=Object.entries(assembly.instance_paths).filter(([,parts])=>JSON.stringify(parts)===JSON.stringify(selectedPath)).map(([id])=>id);
    assert.equal(ids.length,1,'exact nested crank path must resolve uniquely');
    const selectedId=ids[0];
    assert.equal(assembly.instances.find(instance=>instance.id===selectedId).part,'crank');
    const mechanism=manifest.mechanism_reports.four_bar.solution;
    assert.equal(mechanism.solver.status,'fully_constrained');
    assert.equal(mechanism.solver.degrees_of_freedom,0);
    assert.equal(Object.keys(mechanism.pins).length,4);
    assert.equal(Object.keys(mechanism.link_frames).length,4);
    assert.ok(mechanism.branches[0].signed_double_area_mm2>mechanism.branches[0].uncertainty_margin_mm2);
    assert.equal(source,fs.readFileSync(path.join(__dirname,`../../examples/${name}.star`),'utf8'));
    for (const suffix of ['preview.html','gpu.html','stream.html']) {
      const page=await context.newPage(),errors=[],requests=[],failed=[];
      page.on('pageerror',error=>errors.push(error.message));
      page.on('request',request=>requests.push(request.url()));
      page.on('requestfailed',request=>{if(!request.failure()?.errorText.includes('ERR_ABORTED'))failed.push(request.url());});
      const response=await page.goto(`${directory}/${name}.${suffix}`);
      assert.ok(response.ok());
      if(suffix==='preview.html') {
        await page.locator('#occurrence-select').selectOption(selectedId);
        assert.equal(await page.evaluate(()=>selectedInstance),selectedId);
      } else {
        await page.waitForFunction(()=>window.cadGpuStats?.ready===true);
        const index=await page.evaluate(id=>window.compactScene.instances.findIndex(instance=>instance.id===id),selectedId);
        assert.ok(index>=0,'crank must exist in compact occurrence table');
        await page.locator('#occurrence-index').fill(String(index));
        await page.locator('#inspect-index').click();
        assert.equal(await page.evaluate(()=>window.cadGpuStats.selectedInstance),index);
        assert.deepEqual(await page.evaluate(()=>window.cadGpuStats.errors),[]);
        assert.deepEqual(await page.evaluate(()=>window.cadGpuStats.assetErrors),{});
        assert.match(await page.locator('#selection').textContent(),/"crank_unit" \/ "link"/);
      }
      await page.locator('#recipe-toggle').click();
      assert.equal(await page.locator('#recipe-source').textContent(),source);
      const recipeContext=await page.evaluate(()=>JSON.parse(document.getElementById('cad-recipe-data').textContent));
      assert.equal(recipeContext.recipe_context_hash,manifest.recipe_context_hash);
      assert.deepEqual(recipeContext.modules||{},{});
      assert.deepEqual(errors,[]);
      assert.deepEqual(failed,[]);
      assert.equal(requests.filter(url=>/\.brep\.json(?:$|\?)/.test(url)).length,0);
      if(suffix==='preview.html' && process.env.CAD_SCREENSHOT) await page.screenshot({path:process.env.CAD_SCREENSHOT});
      reports.push({viewer:suffix,selected_occurrence:selectedId,instance_path:selectedPath,captured_files:1,exact_geometry_requests:0,errors});
      await page.close();
    }
    console.log(JSON.stringify({valid:true,mechanism_context_hash:manifest.mechanism_context_hash,reports},null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
