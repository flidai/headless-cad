// Inspect actual display motion and source selection without modeling calls.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_adjustment';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/rigid-translation-groups.html`)).ok());
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.ok((await page.request.get(link)).ok(),link);
  const report=await(await page.request.get(`${base}/rigid-translation-groups.json`)).json(),group=report.groups.upper_slot_travel;
  assert.equal(group.members.length,29);assert.equal(group.clearance_check_ids.length,4);
  assert.deepEqual(group.translation_world,[6,0,0]);
  const clearance=await(await page.request.get(`${base}/clearance-checks.json`)).json();
  for(const id of group.clearance_check_ids){const check=clearance.checks[id];assert.equal(check.query.method,'slot_trim_witness');assert.equal(check.sweep.query_count,2);assert.ok(check.sweep.slot_motion_proof);assert.ok(check.lower_bound_mm>.49);}
  assert.ok((await page.goto(`${base}/translation-upper_slot_travel.html`)).ok());
  assert.equal(await page.locator('#translation-inspection').getAttribute('data-error'),null);
  const snapshot=()=>page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];})));
  await page.waitForTimeout(2500);
  const before=await snapshot(),members=new Set(group.members.map(m=>m.instance_id));assert.equal(Object.keys(before).length,30);
  for(const phase of [.125,.5,1]){
   await page.locator('#translation-phase').evaluate((slider,value)=>{slider.value=String(value);slider.dispatchEvent(new Event('input',{bubbles:true}));},phase);
   assert.equal(await page.locator('#translation-inspection').getAttribute('data-phase'),String(phase));
   const after=await snapshot();
   for(const [id,start] of Object.entries(before)){
    assert.deepEqual(after[id].normal,start.normal);
    for(let i=0;i<3;i++)assert.ok(Math.abs(after[id].point[i]-start.point[i]-(members.has(id)?phase*group.translation_world[i]:0))<1e-10,`motion ${id}`);
   }
  }
  await page.locator('#translation-reset').click();assert.deepEqual(await snapshot(),before);
  await page.locator('#recipe-toggle').click();
  const options=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})));
  for(const name of ['adjustment_base','adjustment_bolt_0','shaft','left_cover']){
   const option=options.find(o=>o.text.includes(name));assert.ok(option,name);await page.locator('#occurrence-select').selectOption(option.value);
   assert.ok((await page.locator('#recipe-summary').textContent()).includes(name));
   assert.ok((await page.locator('#recipe-source').textContent()).length>100);
  }
  const bundle=await(await page.request.get(`${base}/recipe.bundle.json`)).json();
  await page.locator('#recipe-module').selectOption('drive_module_rotation.star');
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.modules['drive_module_rotation.star']);
  assert.ok((await page.locator('#recipe-source').textContent()).includes('end_cover_body'));
  await page.locator('#translation-phase').evaluate(slider=>{slider.value='.5';slider.dispatchEvent(new Event('input',{bubbles:true}));});
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.modules['drive_module_rotation.star']);
  const cdp=await page.context().newCDPSession(page),shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync('/tmp/cad-slot-adjustment.png',Buffer.from(shot.data,'base64'));await cdp.detach();
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,moving_occurrences:29,stationary_occurrences:1,continuous_slot_proofs:4,phases:[.125,.5,1],read_only:true,captured_source:true},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
