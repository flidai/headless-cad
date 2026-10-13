// Browser acceptance for both source-controlled, nominally seated slot endpoints.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_DEMO_URL||'http://localhost:8080/tightened_mounting_accepted_demo').replace(/\/$/,'');
const textFor=(bundle,file)=>file===bundle.source_name?bundle.source:bundle.modules[file];
const spanText=(bundle,span)=>Buffer.from(textFor(bundle,span.file),'utf8').subarray(span.start_byte,span.end_byte).toString('utf8');
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha256Start=hash(fs.readFileSync(__filename));

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[],screenshots=[];
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{if(!['GET','HEAD'].includes(request.method()))writes.push(`${request.method()} ${request.url()}`);});
  const landing=await page.goto(`${base}/index.html`);
  assert.ok(landing?.ok(),'accepted demo index loads');
  const evidence=await(await page.request.get(`${base}/evidence.json`)).json();
  assert.equal(evidence.valid,true);
  assert.equal(evidence.durable_jobs,3);
  assert.equal(evidence.offline_replay?.valid,true);
  const previewLinks=await page.locator('a[href$="drive_module_tightened.preview.html"]').evaluateAll(nodes=>nodes.map(n=>n.href));
  assert.equal(previewLinks.length,2,'index links both accepted endpoint previews');
  for(const phase of ['before','after'])assert.ok(previewLinks.some(link=>link.endsWith(`/${phase}/drive_module_tightened.preview.html`)),`${phase} preview link`);
  for(const href of await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href))){
   const response=await page.request.head(href);
   assert.equal(response.status(),200,`demo link ${href}`);
  }
  const sourceByPhase={};
  const provenanceHashes={};
  const sourceBundleHashes={};
  const result=[];
  for(const [phase,endpoint] of [['before',-1],['after',1]]){
   const prefix=`${base}/${phase}`;
   const bundle=await(await page.request.get(`${prefix}/recipe.bundle.json`)).json();
   const provenanceBytes=await(await page.request.get(`${prefix}/source-provenance.json`)).body();
   const provenance=JSON.parse(provenanceBytes.toString('utf8'));
   const manifest=await(await page.request.get(`${prefix}/manifest.json`)).json();
   sourceByPhase[phase]=bundle.source;
   provenanceHashes[phase]=hash(provenanceBytes);
   sourceBundleHashes[phase]=provenance.source_bundle_hash;
   assert.equal(bundle.source_name,'drive_module_tightened.star');
   assert.ok(bundle.source.includes(`mounting_endpoint = param(name = "mounting_endpoint", default = ${endpoint})`),`${phase} endpoint source`);
   assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
   assert.equal(provenance.source_bundle_hash,manifest.source_bundle_hash);
   assert.equal(manifest.source_provenance.artifact,'source-provenance.json');
   for(const [file,item] of Object.entries(provenance.source_inventory)){
    const bytes=Buffer.from(textFor(bundle,file),'utf8');
    assert.equal(bytes.length,item.bytes,`${phase} ${file} captured byte count`);
    assert.equal(hash(bytes),item.sha256,`${phase} ${file} captured digest`);
   }
   assert.equal(manifest.outputs.drive_module_tightened.instances,30);
   assert.equal(manifest.outputs.drive_module_tightened.definitions,14);
   const seats=await(await page.request.get(`${prefix}/nominal-seating-checks.json`)).json();
   assert.deepEqual(seats.checks,manifest.nominal_seating_checks);
   assert.equal(seats.recipe_context_hash,manifest.recipe_context_hash);
   const mountingSeats=Object.entries(seats.checks).filter(([id])=>id.startsWith('tight_'));
   assert.equal(mountingSeats.length,8);
   for(const [id,seat] of mountingSeats){
    assert.equal(seat.status,'passed',`${phase} ${id}`);
    assert.ok(seat.disk_radius_lower_mm>=0.4,`${phase} ${id} protected disk`);
   }
   const gaps=await(await page.request.get(`${prefix}/clearance-checks.json`)).json();
   const mountingGaps=Object.entries(gaps.checks).filter(([id])=>id.startsWith('tight_slot_'));
   assert.equal(mountingGaps.length,4);
   for(const [id,check] of mountingGaps){
    assert.equal(check.status,'passed',`${phase} ${id}`);
    assert.ok(check.lower_bound_mm>=0.49,`${phase} ${id} native gap`);
   }
   const seatPage=await page.goto(`${prefix}/nominal-seating-checks.html`);
   assert.ok(seatPage?.ok(),`${phase} seat report`);
   for(const [id] of mountingSeats)assert.ok((await page.locator('body').innerText()).includes(id),`${phase} report row ${id}`);
   const reportLinks=await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href));
   assert.ok(reportLinks.some(link=>link.endsWith('/drive_module_tightened.preview.html')));
   for(const href of reportLinks)assert.equal((await page.request.head(href)).status(),200,`${phase} report link ${href}`);
   assert.equal((await page.request.head(`${prefix}/drive_module_tightened.step`)).status(),200,`${phase} STEP download`);
   const preview=await page.goto(`${prefix}/drive_module_tightened.preview.html`,{waitUntil:'domcontentloaded'});
   assert.ok(preview?.ok(),`${phase} preview`);
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#recipe-declarations').count(),1);
   assert.equal(await page.locator('#occurrence-select option').count(),31,'30 leaves and one prompt option');
   const selected=[];
   for(const [instance,part] of [['tight_nut_0','nut_envelope'],['tight_bolt_0','clamp_bolt_envelope']]){
    const option=await page.locator('#occurrence-select option').evaluateAll((nodes,name)=>nodes.find(node=>node.textContent.includes(name))?.value,instance);
    assert.ok(option,`${phase} selectable ${instance}`);
    await page.locator('#occurrence-select').selectOption(option);
    const declaration=provenance.parts[part]?.declaration;
    assert.ok(declaration,`${phase} ${part} declaration`);
    await page.waitForFunction(span=>{
     const source=document.querySelector('#recipe-source');
     return source?.dataset.sourceFile===span.file&&source?.dataset.startByte===String(span.start_byte)&&source?.dataset.endByte===String(span.end_byte);
    },declaration);
    assert.equal(declaration.file,'drive_module_rotation.star');
    assert.equal(await page.locator('#recipe-module').inputValue(),declaration.file);
    assert.equal(await page.locator('#recipe-source').textContent(),textFor(bundle,declaration.file));
    assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,declaration));
    assert.ok((await page.locator('#recipe-summary').textContent()).includes(instance));
    const featureId=provenance.part_body_features[part];
    const feature=provenance.features[featureId]?.declaration;
    assert.ok(feature,`${phase} ${part} body feature`);
    await page.locator('#recipe-feature-source').click();
    await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),feature);
    assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,feature));
    selected.push({instance,part,feature:featureId});
   }
   const cdp=await page.context().newCDPSession(page);
   const capture=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
   const screenshot=`/tmp/cad-tightened-${phase}-browser.png`;
   fs.writeFileSync(screenshot,Buffer.from(capture.data,'base64'));
   screenshots.push(screenshot);
   await cdp.detach();
   result.push({phase,endpoint,occurrences:30,seats:mountingSeats.length,gaps:mountingGaps.length,selected});
  }
  assert.notEqual(sourceByPhase.before,sourceByPhase.after,'accepted source revision differs');
  assert.deepEqual(errors,[],'no JavaScript errors');
  assert.deepEqual(writes,[],'viewer and reports make no write requests');
  const scriptSha256End=hash(fs.readFileSync(__filename));
  assert.equal(scriptSha256Start,scriptSha256End,'browser oracle remained frozen');
  console.log(JSON.stringify({valid:true,read_only:true,states:result,
   source_bundle_hash:sourceBundleHashes,source_provenance_sha256:provenanceHashes,screenshots,
   browser_script_sha256_start:scriptSha256Start,browser_script_sha256_end:scriptSha256End},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
