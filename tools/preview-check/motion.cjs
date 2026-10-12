// Recorded motion inspection must remain entirely local and read-only.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
 const directory=`${base}/motion_four_bar`, file='motion-crank_cycle.study.html';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const context=await browser.newContext({viewport:{width:1440,height:1000}});
  const manifestResponse=await context.request.get(`${directory}/manifest.json`);assert.ok(manifestResponse.ok());
  const manifest=await manifestResponse.json();
  const page=await context.newPage(),errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>requests.push({url:r.url(),type:r.resourceType()}));
  await page.goto(base);
  assert.ok(await page.locator(`a[href$="motion_four_bar/${file}"]`).count(),'gallery links recorded study');
  requests.length=0;
  const response=await page.goto(`${directory}/${file}`);assert.ok(response.ok());
  await page.waitForFunction(()=>window.cadMotionStats?.ready);
  const payload=await page.locator('#cad-motion-data').textContent().then(JSON.parse);
  const {study,recipe,mechanism}=payload;
  assert.equal(recipe.recipe_context_hash,manifest.recipe_context_hash);
  assert.ok(study.frames.length>study.requested_frames,'adaptive samples retained');
  assert.equal(study.frames.filter(f=>f.requested_index!==null).length,study.requested_frames);
  assert.ok(Object.keys(recipe.modules).length>0,'captured reusable module');
  assert.equal(await page.locator('#study-source').textContent(),recipe.source);
  for(const [label,source] of Object.entries(recipe.modules)){
   await page.locator('#study-source-picker').selectOption(label);
   assert.equal(await page.locator('#study-source').textContent(),source);
  }
  await page.locator('#study-source-picker').selectOption('');
  const checkFrame=async index=>{
   await page.locator('#study-frame').evaluate((el,index)=>{el.value=index;el.dispatchEvent(new Event('input',{bubbles:true}));},index);
   const frame=study.frames[index];
   const actual=await page.locator('#diagram circle[data-pin]').evaluateAll(nodes=>Object.fromEntries(nodes.map(n=>[n.dataset.pin,[Number(n.getAttribute('cx')),-Number(n.getAttribute('cy'))]])));
   assert.deepEqual(actual,frame.pins);
   const stats=await page.evaluate(()=>window.cadMotionStats);
   assert.equal(stats.index,index);assert.equal(stats.requestedIndex,frame.requested_index);
   const evidence=JSON.parse(await page.locator('#study-evidence').textContent());
   assert.equal(evidence.status,frame.status);assert.equal(evidence.max_residual_mm,frame.max_residual_mm);
  };
  const indices=new Set([0,study.frames.length-1,study.frames.findIndex(f=>f.requested_index===null),...study.frames.flatMap((f,i)=>f.requested_index!==null?[i]:[])]);
  for(const i of indices)await checkFrame(i);
  const link=mechanism.links[mechanism.links.length-1];
  await page.locator(`line[data-link="${link.id}"]`).dispatchEvent('click');
  assert.equal(await page.locator('#study-link').inputValue(),link.id);
  assert.equal((await page.evaluate(()=>window.cadMotionStats)).selectedLink,link.id);
  await checkFrame(0);
  await page.locator('#study-play').click();
  await page.waitForFunction(()=>window.cadMotionStats.index>=2);
  await page.locator('#study-play').click();
  assert.equal(await page.locator('#study-play').textContent(),'Play samples');
  const paused=await page.evaluate(()=>window.cadMotionStats.index);
  await page.waitForTimeout(300);
  assert.equal(await page.evaluate(()=>window.cadMotionStats.index),paused);
  await checkFrame(paused);
  assert.deepEqual(errors,[]);
  assert.equal(requests.filter(r=>['fetch','xhr'].includes(r.type)).length,0);
  assert.equal(requests.filter(r=>/\.brep\.json(?:$|\?)/.test(r.url)).length,0);
  assert.equal(requests.filter(r=>r.type==='document').length,1);
  if(process.env.CAD_SCREENSHOT)await page.screenshot({path:process.env.CAD_SCREENSHOT});
  console.log(JSON.stringify({valid:true,frames:study.frames.length,requested_frames:study.requested_frames,scrubbed_indices:[...indices],source_context:recipe.recipe_context_hash,captured_modules:Object.keys(recipe.modules),fetch_or_xhr:0,exact_body_requests:0,errors},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
