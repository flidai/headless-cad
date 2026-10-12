// Paged metadata scale and identity acceptance; SwiftShader is not a hardware gate.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const url=process.env.CAD_PAGED_URL||'http://localhost:8080/paged_scale/paged_workcell.paged.html';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
  const errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r.url()));
  const response=await page.goto(url);assert.ok(response.ok());
  await page.waitForFunction(()=>window.cadPagedStats?.ready);
  const first=await page.evaluate(()=>window.cadPaged.snapshot());
  assert.ok(first.metadata.entries.length<100000,'first coverage cannot expand entire assembly');
  await page.evaluate(()=>window.cadPaged.settle());
  const overview=await page.evaluate(()=>window.cadPaged.snapshot());
  assert.equal(overview.stats.totalOccurrences,100000);
  function bounded(s){
   assert.ok(s.metadata.stats.residentRecords<=2048);
   assert.ok(s.metadata.stats.residentPages<=256);
   assert.ok(s.metadata.stats.residentBytes+s.metadata.stats.reservedBytes<=8*1048576);
   assert.ok(s.stats.residentMeshBytes+s.stats.reservedMeshBytes<=64*1048576);
   assert.ok(s.stats.instanceBufferBytes<=(2048+512)*44);
   assert.deepEqual(s.metadata.stats.errors,[]);
   assert.deepEqual(s.stats.errors,[]);
  }
  bounded(overview);
  assert.ok(overview.metadata.proxies.length>0,'overview retains explicit aggregate coverage');
  const index=await page.locator('#cad-metadata-data').textContent().then(JSON.parse);
  const recipe=await page.locator('#cad-recipe-data').textContent().then(JSON.parse);
  assert.equal(index.recipe_context_hash,recipe.recipe_context_hash);
  assert.ok(JSON.stringify(index).length<2048);
  const captures=[];
  for(const [station,tool,point] of [[0,0,[4,4,6]],[999,99,[5572,3472,6]],[0,0,[4,4,6]]]){
   await page.evaluate(point=>{window.cadPaged.setView('top');window.cadPaged.setCamera({target:point,halfHeight:15});},point);
   await page.evaluate(()=>window.cadPaged.settle());
   const snapshot=await page.evaluate(()=>window.cadPaged.snapshot());bounded(snapshot);
   const hit=await page.evaluate(point=>window.cadPaged.pick(...window.cadPaged.project(point)),point);
   assert.ok(hit&&!hit.unresolved,'focused surface must have resolved identity');
   assert.deepEqual(hit.path,[`station_${station}`,`tool_${tool}`]);
   assert.ok(hit.face>=0);
   assert.equal(await page.locator('#source').textContent(),recipe.source);
   assert.ok(await page.locator('#recipe-panel').isVisible());
   captures.push({hit,stats:snapshot.stats,metadata:snapshot.metadata.stats});
  }
  await page.screenshot({path:'/tmp/cad-paged-viewer.png'});
  assert.deepEqual(errors,[]);
  assert.equal(requests.filter(u=>/\.(brep|assembly|stream|display)\.json$/.test(u)&&!/(mesh|metadata)-[a-f0-9]{64}\.display\.json$/.test(u)).length,0,'no eager scene or exact-body fetch');
  const bad=await browser.newPage();
  await bad.route(`**/${index.root.url}`,route=>route.fulfill({status:200,contentType:'application/json',body:'{}'}));
  await bad.goto(url);await bad.waitForFunction(()=>window.cadPagedStats?.metadata?.errors?.length>0);
  const failed=await bad.evaluate(()=>window.cadPaged.snapshot());
  assert.equal(failed.metadata.entries.length,0);assert.ok(failed.metadata.proxies.length>0);
  assert.match(await bad.locator('#status').textContent(),/metadata.*(fail|unavailable)/i);
  const modules=await browser.newPage();
  await modules.goto(new URL('/paged_modules/modular_linear_stage.paged.html',url).href);
  await modules.waitForFunction(()=>window.cadPagedStats?.ready);
  await modules.evaluate(()=>window.cadPaged.settle());
  const captured=await modules.locator('#cad-recipe-data').textContent().then(JSON.parse);
  assert.ok(Object.keys(captured.modules).length>0);
  await modules.locator('#recipe-toggle').click();
  for(const [name,source] of Object.entries(captured.modules)){
   await modules.locator('#module').selectOption(name);
   assert.equal(await modules.locator('#source').textContent(),source);
  }
  const legacy=await browser.newPage();
  await legacy.goto(new URL('/paged_modules/modular_linear_stage.gpu.html',url).href);
  await legacy.waitForFunction(()=>window.cadGpuStats?.ready);
  for(const point of [[40,45,28],[28,8,28]]){
   const newHit=await modules.evaluate(async point=>{cadPaged.setView('top');cadPaged.setCamera({target:point,halfHeight:12});await cadPaged.settle();return cadPaged.pick(...cadPaged.project(point));},point);
   const oldHit=await legacy.evaluate(point=>{cadGpuTest.setView('top');cadGpuTest.setCamera({target:point,halfHeight:12});cadGpuTest.draw();return cadGpuTest.pick(...cadGpuTest.project(point));},point);
   assert.deepEqual({instance:newHit.instance,face:newHit.face},oldHit,'solid and through-hole picks agree with existing viewer');
  }
  assert.deepEqual(await modules.evaluate(()=>cadPagedStats.errors),[]);
  const mismatched=await browser.newPage();
  await mismatched.route(url,async route=>{
   const upstream=await route.fetch();
   let html=await upstream.text();
   const sourceTag=html.match(/<script id="cad-recipe-data" type="application\/json">(.*?)<\/script>/s);
   const wrong=JSON.parse(sourceTag[1]);wrong.recipe_context_hash='wrong-generation';
   html=html.replace(sourceTag[0],`<script id="cad-recipe-data" type="application/json">${JSON.stringify(wrong)}</script>`);
   await route.fulfill({response:upstream,body:html});
  });
  await mismatched.goto(url);
  await mismatched.waitForFunction(()=>window.cadPagedStats?.errors?.length>0);
  assert.equal(await mismatched.evaluate(()=>window.cadPagedStats.ready),false);
  console.log(JSON.stringify({valid:true,url,total:100000,first_records:first.metadata.entries.length,overview:overview.stats,captures,requests:requests.length,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
