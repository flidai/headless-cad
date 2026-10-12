// Exact native keyseat face picking and captured-source inspection.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,''),url=`${base}/keyed_shaft`;
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  const manifest=await(await page.request.get(`${url}/manifest.json`)).json();
  const floor=manifest.face_assertions.seat_floor.witnesses[0].target_face;
  assert.ok((await page.goto(`${url}/keyed_shaft.preview.html`)).ok());
  await page.locator('[data-view="right"]').click();
  const picked=await page.evaluate(()=>{
   const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
   const project=p=>{const d=sub(p,center);return[dot(d,right),-dot(d,up)]};
   const extent=[0,0];for(const f of scene.faces)for(const p of f.points){const q=project(p);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));}
   const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1));
   const q=project([4,0,17]);select(width/2+q[0]*scale,height/2+20+q[1]*scale);
   const face=scene.faces.find(f=>f.id===selected);return face?.semantic_face_id??face?.local_face_id??face?.id;
  });
  assert.equal(picked,floor,'floor must be visible and retain its native identity');
  await page.locator('#recipe-toggle').click();const source=await(await page.request.get(`${url}/${manifest.recipe_source.artifact}`)).text();
  assert.equal(await page.locator('#recipe-source').textContent(),source);assert.match(source,/keyseat_floor/);assert.deepEqual(errors,[]);
  await page.locator('[data-view="iso"]').click();
  const cdp=await page.context().newCDPSession(page),shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync('/tmp/cad-keyseat.png',Buffer.from(shot.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,floor_native_face:floor,captured_source:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
