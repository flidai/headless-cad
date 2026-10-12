// Static parallel annulus clearance evidence; does not execute modeling tools.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,''),dir=`${base}/parallel_clearance`;
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push({type:r.resourceType(),url:r.url()}));
  const r=await page.goto(`${dir}/clearance-checks.html`);assert.ok(r.ok());assert.equal(await page.locator('script').count(),0);
  await page.locator('details summary').click();const inline=JSON.parse(await page.locator('details pre').textContent());assert.equal(inline.schema_version,2);assert.equal(inline.kind,'native_geometric_clearance');
  const get=async name=>{const response=await page.request.get(`${dir}/${name}`);assert.ok(response.ok());return response.json();};
  assert.deepEqual(inline,await get('clearance-checks.json'));const manifest=await get('manifest.json');
  assert.match(inline.recipe_context_hash,/^[0-9a-f]{64}$/);assert.equal(inline.recipe_context_hash,manifest.recipe_context_hash);assert.deepEqual(inline.checks,manifest.clearance_checks);
  assert.deepEqual(Object.keys(inline.checks).sort(),['eccentric_shaft','offset_sleeve']);
  for(const [id,check] of Object.entries(inline.checks)){
   assert.equal(check.status,'passed');assert.equal(check.query.method,'parallel_annular');assert.equal(check.declaration.min_clearance,.2);assert.deepEqual(check.left.instance_path,['cell',id==='eccentric_shaft'?'shaft':'bearing']);assert.deepEqual(check.right.instance_path,['cell',id==='eccentric_shaft'?'bearing':'spacer']);const expected=id==='eccentric_shaft'?.75:6;assert.ok(check.lower_bound_mm<=expected&&expected<=check.upper_bound_mm);const cells=await page.locator('tbody tr').filter({hasText:id}).locator('td').allTextContents();assert.equal(cells[0],id);assert.equal(cells[2],'passed');assert.equal(cells[5],'Parallel annuli');
   const interval=JSON.parse(cells[3]);assert.deepEqual(interval,[check.lower_bound_mm,check.upper_bound_mm]);assert.ok(interval.every(Number.isFinite)&&interval[0]>=check.declaration.min_clearance&&interval[1]>=interval[0]&&interval[1]-interval[0]<1e-4);
   assert.equal(Number(cells[4]),check.declaration.min_clearance);
   for(const side of ['left','right']){const part=check[side];assert.ok(part.instance_path.length>0&&part.instance_id&&part.part_id);assert.match(part.part_content_hash,/^[0-9a-f]{64}$/);assert.ok(part.frame.translation.every(Number.isFinite)&&part.frame.rotation.every(Number.isFinite));assert.ok(check.query[`${side}_point`].every(Number.isFinite));}
  }
  const source=manifest.recipe_source.artifact;assert.ok(typeof source==='string'&&source.length);assert.equal(await page.locator(`a[href="${source}"]`).count(),1);assert.ok((await page.request.get(`${dir}/${source}`)).ok());
  const links=await page.locator('a[href$=".preview.html"]').evaluateAll(nodes=>nodes.map(n=>n.getAttribute('href')));assert.ok(links.length);for(const link of links)assert.ok((await page.request.get(`${dir}/${link}`)).ok());
  assert.equal(requests.filter(r=>['fetch','xhr','script'].includes(r.type)||/\.brep\.json/.test(r.url)).length,0);assert.deepEqual(errors,[]);
  const cdp=await page.context().newCDPSession(page),image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-parallel-clearance.png',Buffer.from(image.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,checks:Object.keys(inline.checks),recipe_context_hash:inline.recipe_context_hash,static_no_script:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
