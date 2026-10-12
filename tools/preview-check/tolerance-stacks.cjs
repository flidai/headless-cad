// Static stack evidence inspection, with no modeling or script execution.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,''),dir=`${base}/bearing_clearance`;
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push({type:r.resourceType(),url:r.url()}));
  const response=await page.goto(`${dir}/tolerance-stacks.html`);assert.ok(response.ok());
  assert.equal(await page.locator('script').count(),0);
  const cells=await page.locator('tbody tr').filter({hasText:'diametral_clearance'}).locator('td').allTextContents();
  assert.equal(cells[0],'diametral_clearance');assert.equal(cells[1],'passed');assert.ok(Math.abs(Number(cells[2])-.4)<1e-12);
  const interval=JSON.parse(cells[3]),required=JSON.parse(cells[4]);assert.ok(Math.abs(interval[0]-.4)<1e-12&&Math.abs(interval[1]-.44)<1e-12);assert.deepEqual(required,[.39,.45]);
  await page.locator('details summary').click();
  const inline=JSON.parse(await page.locator('details pre').textContent());
  for(const name of ['tolerance-stacks.json','recipe.bundle.json']){
   const link=page.locator(`a[href="${name}"]`);assert.equal(await link.count(),1);
   const r=await page.request.get(new URL(await link.getAttribute('href'),page.url()).href);assert.ok(r.ok());
   const data=await r.json();if(name==='tolerance-stacks.json')assert.deepEqual(inline,data);
  }
  const m=await page.request.get(`${dir}/manifest.json`),manifest=await m.json();assert.match(inline.recipe_context_hash,/^[0-9a-f]{64}$/);assert.match(manifest.recipe_context_hash,/^[0-9a-f]{64}$/);assert.equal(inline.recipe_context_hash,manifest.recipe_context_hash);
  const reference=inline.section_reports.pilot_limits;assert.equal(reference.file,'section-pilot_limits.json');assert.equal(reference.algorithm,'blake3');assert.match(reference.digest,/^[0-9a-f]{64}$/);
  const sectionResponse=await page.request.get(`${dir}/${reference.file}`);assert.ok(sectionResponse.ok());const section=await sectionResponse.json();for(const key of ['recipe_source_hash','source_bundle_hash']){assert.match(section.snapshot[key],/^[0-9a-f]{64}$/);assert.match(manifest[key],/^[0-9a-f]{64}$/);assert.equal(section.snapshot[key],manifest[key]);}
  const terms=inline.stacks.diametral_clearance.declaration.terms;assert.equal(terms.length,2);assert.ok(terms.every(t=>t.view==='pilot_limits'));assert.deepEqual(terms.map(t=>t.dimension).sort(),['bore_size','pin_size']);
  assert.equal(requests.filter(r=>['fetch','xhr','script'].includes(r.type)||/\.brep\.json/.test(r.url)).length,0);assert.deepEqual(errors,[]);
  const cdp=await page.context().newCDPSession(page),image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-bearing-clearance.png',Buffer.from(image.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,nominal:Number(cells[2]),interval,required,recipe_context_hash:inline.recipe_context_hash,section_reference:reference,static_no_script:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
