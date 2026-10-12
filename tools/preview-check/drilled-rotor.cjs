// Source-bound rigid display motion, with no authoring or model execution.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drilled_rotor_fixture';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/rigid-rotation-groups.html`)).ok());
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.ok((await page.request.get(link)).ok(),link);
  const evidence=await(await page.request.get(`${base}/rigid-rotation-groups.json`)).json();
  const group=evidence.groups.rotor_rotation;assert.equal(group.members.length,1);assert.equal(group.coverage,'named_pairs_only');assert.equal(group.clearance_check_ids.length,1);
  const source=await(await page.request.get(`${base}/recipe.star`)).text();
  assert.ok((await page.goto(`${base}/rotation-rotor_rotation.html`)).ok());
  assert.equal(await page.locator('#rotation-inspection').getAttribute('data-error'),null);
  const before=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{
   const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];
  })));
  const members=new Set(group.members.map(member=>member.instance_id)),axis=group.rotation_world.axis,pivot=group.rotation_world.pivot;
  function turn(vector,angle){const [x,y,z]=axis,[a,b,c]=vector,cross=[y*c-z*b,z*a-x*c,x*b-y*a],dot=x*a+y*b+z*c;return vector.map((v,i)=>v*Math.cos(angle)+cross[i]*Math.sin(angle)+axis[i]*dot*(1-Math.cos(angle)));}
  for(const phase of [.075,.125,.25,.5,1]){
   await page.locator('#rotation-phase').evaluate((slider,value)=>{slider.value=String(value);slider.dispatchEvent(new Event('input',{bubbles:true}));},phase);
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),String(phase));
   const after=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];})));
   for(const [id,start] of Object.entries(before)){
    if(!members.has(id)){assert.deepEqual(after[id],start,`stationary ${id}`);continue;}
    const expected=turn(start.point.map((v,i)=>v-pivot[i]),phase*group.rotation_world.angle_radians).map((v,i)=>v+pivot[i]);
    const normal=turn(start.normal,phase*group.rotation_world.angle_radians);
    for(let i=0;i<3;i++){assert.ok(Math.abs(after[id].point[i]-expected[i])<1e-10,`point ${id}`);assert.ok(Math.abs(after[id].normal[i]-normal[i])<1e-10,`normal ${id}`);}
   }
  }
  await page.locator('#rotation-reset').click();
  const reset=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];})));
  assert.deepEqual(reset,before);
  await page.locator('#recipe-toggle').click();
  const options=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})));
  for(const name of ['rotor','roller']){
   const option=options.find(o=>o.text.includes(name));assert.ok(option,name);await page.locator('#occurrence-select').selectOption(option.value);
   assert.equal(await page.locator('#recipe-source').textContent(),source);assert.ok((await page.locator('#recipe-summary').textContent()).includes(name));
  }
  await page.locator('#rotation-phase').evaluate(slider=>{slider.value='.075';slider.dispatchEvent(new Event('input',{bubbles:true}));});
  await page.waitForTimeout(1200);
  assert.equal(await page.locator('.badge').textContent(),'READ-ONLY MOTION');
  assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),'0.075');
  assert.equal(await page.locator('#recipe-source').textContent(),source);
  const capture=async path=>{const cdp=await page.context().newCDPSession(page),shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(path,Buffer.from(shot.data,'base64'));await cdp.detach();};
  await capture('/tmp/cad-drilled-rotor.png');
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,moving_occurrences:1,stationary_occurrences:1,phases:[.075,.125,.25,.5,1],captured_source:true,read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
