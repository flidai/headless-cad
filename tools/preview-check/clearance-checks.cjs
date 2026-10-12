const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,''),dir=`${base}/clearance_station`;
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push({type:r.resourceType(),url:r.url()}));
  const r=await page.goto(`${dir}/clearance-checks.html`);assert.ok(r.ok());assert.equal(await page.locator('script').count(),0);
  const cells=await page.locator('tbody tr').filter({hasText:'jaw_separation'}).locator('td').allTextContents();
  assert.equal(cells[0],'jaw_separation');assert.ok(cells[1].includes('cell / fixed')&&cells[1].includes('cell / moving'));assert.equal(cells[2],'passed');assert.equal(Number(cells[4]),2);
  const interval=JSON.parse(cells[3]);assert.ok(interval.every(Number.isFinite)&&interval[0]>=2&&interval[1]>=interval[0]&&interval[1]-interval[0]<1e-5);
  await page.locator('details summary').click();const inline=JSON.parse(await page.locator('details pre').textContent());
  const get=async name=>{const response=await page.request.get(`${dir}/${name}`);assert.ok(response.ok());return response.json();};
  assert.deepEqual(inline,await get('clearance-checks.json'));const manifest=await get('manifest.json');
  assert.match(inline.recipe_context_hash,/^[0-9a-f]{64}$/);assert.equal(inline.recipe_context_hash,manifest.recipe_context_hash);
  assert.deepEqual(inline.checks,manifest.clearance_checks);const check=inline.checks.jaw_separation;
  assert.deepEqual(interval,[check.lower_bound_mm,check.upper_bound_mm]);assert.ok(check.lower_bound_mm<=check.query.lower_bound_mm&&check.upper_bound_mm>=check.query.upper_bound_mm);
  const point=p=>assert.ok(Array.isArray(p)&&p.length===3&&p.every(Number.isFinite));point(check.query.left_point);point(check.query.right_point);point(check.query.axis);
  const distance=Math.hypot(...check.query.left_point.map((v,i)=>v-check.query.right_point[i]));assert.ok(Math.abs(distance-check.query.upper_bound_mm)<1e-5);
  for(const [side,leaf] of [['left','fixed'],['right','moving']]){const part=check[side];assert.deepEqual(part.instance_path,['cell',leaf]);assert.ok(part.instance_id&&part.part_id&&part.feature_id);assert.match(part.part_content_hash,/^[0-9a-f]{64}$/);point(part.frame.translation);assert.ok(part.frame.rotation.length===4&&part.frame.rotation.every(Number.isFinite));}
  const sourceName=manifest.recipe_source.artifact;assert.equal(typeof sourceName,'string');
  assert.equal(await page.locator(`a[href="${sourceName}"]`).count(),1);assert.ok((await page.request.get(`${dir}/${sourceName}`)).ok());
  const preview=page.locator('a[href="clearance_station.preview.html"]');assert.equal(await preview.count(),1);assert.ok((await page.request.get(`${dir}/${await preview.getAttribute('href')}`)).ok());
  assert.equal(requests.filter(r=>['fetch','xhr','script'].includes(r.type)||/\.brep\.json/.test(r.url)).length,0);assert.deepEqual(errors,[]);
  const cdp=await page.context().newCDPSession(page),image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-clearance-station.png',Buffer.from(image.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,interval,required:2,recipe_context_hash:inline.recipe_context_hash,static_no_script:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
