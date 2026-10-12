// Inspect a completed source-revision walkthrough without executing modeling jobs.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_DEMO_URL||'http://localhost:8080/keyed_drive_demo').replace(/\/$/,'');
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/index.html`)).ok());
  assert.match(await page.locator('h1').textContent(),/drive-module revision/);
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.ok((await page.request.get(link)).ok(),link);
  const record=await(await page.request.get(`${base}/walkthrough.json`)).json();
  assert.equal(record.kind,'scripted_source_revision');assert.equal(record.rejected_edit_preserved_generation,true);assert.equal(record.captured_source_replayed,true);
  assert.notEqual(record.before_recipe_context_hash,record.after_recipe_context_hash);
  for(const [name,diameter] of [['before',12],['after',14]]){
   const source=await(await page.request.get(`${base}/${name}/recipe.star`)).text();
   assert.ok((await page.goto(`${base}/${name}/drive_module_release.preview.html`)).ok());
   const value=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.find(n=>/shaft/.test(n.textContent))?.value);assert.ok(value,'shaft occurrence missing');
   await page.locator('#occurrence-select').selectOption(value);
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#recipe-source').textContent(),source);
   assert.match(await page.locator('#recipe-summary').textContent(),/shaft/);
   assert.ok(source.includes(`param(name = "shaft_diameter", default = ${diameter})`));
   const key=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.find(n=>/parallel_key/.test(n.textContent))?.value);assert.ok(key,'actual key missing');
   await page.locator('#occurrence-select').selectOption(key);
   assert.match(await page.locator('#recipe-summary').textContent(),/parallel_key/);
   assert.equal(await page.locator('#recipe-source').textContent(),source);
   const manifest=await(await page.request.get(`${base}/${name}/manifest.json`)).json();
   assert.equal(manifest.outputs.drive_module_release.instances,20);
   assert.equal(manifest.outputs.drive_module_release.definitions,10);
   assert.equal(Object.keys(manifest.clearance_checks).length,7);
   for(const check of Object.values(manifest.clearance_checks)){
    assert.equal(check.status,'passed');assert.equal(check.query.method,'native_trim_witness');
   }

  }
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  const cdp=await page.context().newCDPSession(page),shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync('/tmp/cad-keyed-drive-demo.png',Buffer.from(shot.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,source_revisions:[12,14],read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
