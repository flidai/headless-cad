// Check captured module inspection on every supported assembly viewer.
const {chromium} = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const base = process.env.CAD_BASE_URL || 'http://localhost:8080';
  const browser = await chromium.launch({executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const reports=[];
  try {
    for (const suffix of ['preview.html','gpu.html','stream.html']) {
      const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[];
      page.on('pageerror',e=>errors.push(e.message));
      page.on('request',r=>requests.push(r.url()));
      const response=await page.goto(`${base}/modular_linear_stage/modular_linear_stage.${suffix}`);
      assert.ok(response.ok());
      if(suffix!=='preview.html') await page.waitForFunction(()=>window.cadGpuStats?.ready===true);
      else await page.locator('#occurrence-select').selectOption('carriage_front');
      await page.locator('#recipe-toggle').click();
      assert.equal(await page.locator('#recipe-module option').count(),3);
      for(const label of ['recipe_modules/drive_parts.star','recipe_modules/stock.star']) {
        await page.locator('#recipe-module').selectOption(label);
        assert.equal(await page.locator('#recipe-source').textContent(),fs.readFileSync(path.join(__dirname,'../../examples',label),'utf8'));
      }
      await page.locator('#recipe-module').selectOption('');
      assert.equal(await page.locator('#recipe-source').textContent(),fs.readFileSync(path.join(__dirname,'../../examples/modular_linear_stage.star'),'utf8'));
      assert.deepEqual(errors,[]);
      assert.equal(requests.filter(url=>/\.brep\.json(?:$|\?)/.test(url)).length,0);
      if(suffix==='preview.html' && process.env.CAD_SCREENSHOT) {
        await page.locator('#recipe-module').selectOption('recipe_modules/drive_parts.star');
        await page.screenshot({path:process.env.CAD_SCREENSHOT});
      }
      reports.push({viewer:suffix,captured_files:3,exact_geometry_requests:0,errors});
      await page.close();
    }
    console.log(JSON.stringify({valid:true,reports},null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
