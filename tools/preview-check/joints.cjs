// Inspect prescribed-joint assembly occurrences and captured recipe modules.
// This checks the read-only viewers; native joint certification lives in the build.
const {chromium} = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const base = (process.env.CAD_BASE_URL || 'http://localhost:8080').replace(/\/$/, '');
  const name = 'jointed_linear_stage';
  const directory = `${base}/${name}`;
  const browser = await chromium.launch({executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const reports=[];
  try {
    const context=await browser.newContext({viewport:{width:1440,height:1000}});
    const read=async file=>{const response=await context.request.get(`${directory}/${file}`);assert.ok(response.ok(),`${file}: ${response.status()}`);return response.json();};
    const manifest=await read('manifest.json');
    const assembly=await read(`${name}.assembly.json`);
    const bundle=await read('recipe.bundle.json');
    const armPath=['stage','head','arm'];
    const ids=Object.entries(assembly.instance_paths).filter(([,parts])=>JSON.stringify(parts)===JSON.stringify(armPath)).map(([id])=>id);
    assert.equal(ids.length,1,'exact nested arm path must resolve uniquely');
    const armId=ids[0];
    assert.equal(assembly.instances.find(instance=>instance.id===armId).part,'arm');
    assert.equal(manifest.outputs[name].joints.length,2);
    assert.equal(bundle.source,fs.readFileSync(path.join(__dirname,`../../examples/${name}.star`),'utf8'));
    for(const [label,source] of Object.entries(bundle.modules)) assert.equal(source,fs.readFileSync(path.join(__dirname,'../../examples',label),'utf8'));
    for (const suffix of ['preview.html','gpu.html','stream.html']) {
      const page=await context.newPage(),errors=[],requests=[],failed=[];
      page.on('pageerror',error=>errors.push(error.message));
      page.on('request',request=>requests.push(request.url()));
      page.on('requestfailed',request=>{if(!request.failure()?.errorText.includes('ERR_ABORTED'))failed.push(request.url());});
      const response=await page.goto(`${directory}/${name}.${suffix}`);
      assert.ok(response.ok());
      if(suffix==='preview.html') {
        await page.locator('#occurrence-select').selectOption(armId);
        assert.equal(await page.evaluate(()=>selectedInstance),armId);
      } else {
        await page.waitForFunction(()=>window.cadGpuStats?.ready===true);
        const index=await page.evaluate(id=>window.compactScene.instances.findIndex(instance=>instance.id===id),armId);
        assert.ok(index>=0,'arm must exist in compact occurrence table');
        await page.locator('#occurrence-index').fill(String(index));
        await page.locator('#inspect-index').click();
        assert.equal(await page.evaluate(()=>window.cadGpuStats.selectedInstance),index);
        assert.deepEqual(await page.evaluate(()=>window.cadGpuStats.errors),[]);
        assert.deepEqual(await page.evaluate(()=>window.cadGpuStats.assetErrors),{});
        assert.match(await page.locator('#selection').textContent(),/"stage" \/ "head" \/ "arm"/);
      }
      await page.locator('#recipe-toggle').click();
      assert.equal(await page.locator('#recipe-module option').count(),1+Object.keys(bundle.modules).length);
      for(const [label,source] of Object.entries(bundle.modules)) {
        await page.locator('#recipe-module').selectOption(label);
        assert.equal(await page.locator('#recipe-source').textContent(),source);
      }
      await page.locator('#recipe-module').selectOption('');
      assert.equal(await page.locator('#recipe-source').textContent(),bundle.source);
      const recipeContext=await page.evaluate(()=>JSON.parse(document.getElementById('cad-recipe-data').textContent));
      assert.equal(recipeContext.source_bundle_hash,manifest.source_bundle_hash);
      assert.deepEqual(recipeContext.modules,bundle.modules);
      assert.deepEqual(errors,[]);
      assert.deepEqual(failed,[]);
      assert.equal(requests.filter(url=>/\.brep\.json(?:$|\?)/.test(url)).length,0);
      if(suffix==='preview.html' && process.env.CAD_SCREENSHOT) await page.screenshot({path:process.env.CAD_SCREENSHOT});
      reports.push({viewer:suffix,selected_occurrence:armId,instance_path:armPath,captured_files:1+Object.keys(bundle.modules).length,exact_geometry_requests:0,errors});
      await page.close();
    }
    console.log(JSON.stringify({valid:true,source_bundle_hash:manifest.source_bundle_hash,reports},null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
