// Browser acceptance for two durable source-owned pocketed mounting releases.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_DEMO_URL||'http://localhost:8080/agent_pocketed_mounting_demo').replace(/\/$/,'');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const textFor=(bundle,file)=>file===bundle.source_name?bundle.source:bundle.modules[file];
const spanText=(bundle,span)=>Buffer.from(textFor(bundle,span.file),'utf8')
 .subarray(span.start_byte,span.end_byte).toString('utf8');

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[],screenshots=[];
 try{
  const page=await browser.newPage({viewport:{width:1500,height:1050}});
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{
   if(!['GET','HEAD'].includes(request.method()))writes.push(`${request.method()} ${request.url()}`);
  });
  const index=await page.goto(`${base}/index.html`,{waitUntil:'domcontentloaded'});
  assert.ok(index?.ok(),'accepted demo index loads');
  const indexText=await page.locator('body').innerText();
  assert.ok(/rejected/i.test(indexText),'index explains rejected revision');
  const indexLinks=await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href));
  for(const phase of ['before','after']){
   assert.ok(indexLinks.some(url=>url.endsWith(`/${phase}/drive_module_tightened.preview.html`)),
    `${phase} accepted preview link`);
  }
  assert.ok(indexLinks.some(url=>url.endsWith('/evidence.json')),'rejection and replay evidence link');
  for(const href of indexLinks)assert.equal((await page.request.head(href)).status(),200,`index link ${href}`);
  const evidenceBytes=await(await page.request.get(`${base}/evidence.json`)).body();
  const evidenceHash=sha(evidenceBytes);
  const evidence=JSON.parse(evidenceBytes.toString('utf8'));
  assert.equal(evidence.valid,true);
  assert.equal(evidence.durable_jobs,3);
  assert.equal(evidence.offline_replay?.valid,true);
  assert.equal(evidence.accepted_revisions?.length,2);
  assert.equal(evidence.parameter_overrides?.base_pocket_depth,1);
  assert.ok(evidence.rejected_unseated_nut?.worker?.result?.code==='nominal_seating_check_failed',
   'rejected unseated nut is a native seating refusal');
  const state={},results=[];
  for(const [phase,endpoint] of [['before',-1],['after',1]]){
   const prefix=`${base}/${phase}`;
   const bundleBytes=await(await page.request.get(`${prefix}/recipe.bundle.json`)).body();
   const manifestBytes=await(await page.request.get(`${prefix}/manifest.json`)).body();
   const bundle=JSON.parse(bundleBytes.toString('utf8'));
   const manifest=JSON.parse(manifestBytes.toString('utf8'));
   const provenance=await(await page.request.get(`${prefix}/source-provenance.json`)).json();
   assert.equal(bundle.source_name,'drive_module_tightened.star');
   assert.ok(bundle.source.includes(`mounting_endpoint = param(name = "mounting_endpoint", default = ${endpoint})`),
    `${phase} exact authored endpoint source`);
   assert.ok(bundle.modules['drive_module_rotation.star']?.includes('base_deep_relief'),
    `${phase} captured pocketed loaded module`);
   assert.equal(manifest.recipe_source.parameter_overrides.base_pocket_depth,1,
    `${phase} exact evaluated pocket depth override`);
   assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
   assert.equal(provenance.source_bundle_hash,manifest.source_bundle_hash);
   for(const [file,item] of Object.entries(provenance.source_inventory)){
    const bytes=Buffer.from(textFor(bundle,file),'utf8');
    assert.equal(bytes.length,item.bytes,`${phase} ${file} byte count`);
    assert.equal(sha(bytes),item.sha256,`${phase} ${file} digest`);
   }
   const basePart=evidence.mechanical_fixture[phase].pocketed_base;
   assert.equal(basePart.feature_id,'base_deep_relief',`${phase} actual pocketed base`);
   assert.equal(manifest.feature_hashes.base_deep_relief,basePart.content_hash,
    `${phase} native finished-base feature hash`);
   assert.ok(basePart.native_asset.startsWith('definition-')&&
             basePart.native_asset.endsWith('.brep.json'),`${phase} native base asset`);
   const baseAsset=await(await page.request.get(`${prefix}/${basePart.native_asset}`)).body();
   assert.equal(sha(baseAsset),basePart.native_asset_sha256,`${phase} captured native base bytes`);
   assert.ok(Math.abs(basePart.material_volume_mm3-69863.17699835308)<1e-6,
    `${phase} independently established pocketed base volume`);
   for(const [name,value] of Object.entries({base_shallow_depth:1,base_deep_depth:2,
                                             base_remaining_web:4})){
    const dimension=basePart.floor_dimensions[name];
    assert.equal(dimension.status,'passed',`${phase} ${name} native floor witness`);
    assert.ok(Math.abs(dimension.value_mm-value)<1e-8,`${phase} ${name} measured value`);
    assert.equal(dimension.from.part_content_hash,basePart.content_hash);
    assert.equal(dimension.to.part_content_hash,basePart.content_hash);
   }
   assert.equal(basePart.shallow_floor_lineage.status,'resolved',
    `${phase} first pocket floor retained through second cut`);
   assert.equal(basePart.shallow_floor_lineage.witnesses.length,1);
   assert.notEqual(basePart.shallow_floor_lineage.witnesses[0].anchor_faces[0],
                   basePart.shallow_floor_lineage.witnesses[0].target_face);
   const assembly=manifest.outputs.drive_module_tightened;
   assert.equal(assembly.instances,30,`${phase} 30 actual occurrences`);
   assert.equal(assembly.definitions,14,`${phase} 14 shared definitions`);
   assert.equal((await page.request.head(`${prefix}/drive_module_tightened.step`)).status(),200,
    `${phase} complete assembly STEP export`);
   const seating=await(await page.request.get(`${prefix}/nominal-seating-checks.json`)).json();
   const clearance=await(await page.request.get(`${prefix}/clearance-checks.json`)).json();
   assert.deepEqual(seating.checks,manifest.nominal_seating_checks);
   assert.deepEqual(clearance.checks,manifest.clearance_checks);
   assert.equal(seating.recipe_context_hash,manifest.recipe_context_hash);
   assert.equal(clearance.recipe_context_hash,manifest.recipe_context_hash);
   const seats=Object.entries(seating.checks).filter(([id])=>/^tight_(bolt|nut)_\d+_.*_seat$/.test(id));
   assert.equal(seats.length,8,`${phase} eight source-named seats`);
   for(const [id,seat] of seats){
    assert.equal(seat.status,'passed',`${phase} ${id}`);
    assert.equal(seat.assembly_content_hash,assembly.content_hash);
    assert.ok(seat.disk_radius_lower_mm>=.4,`${phase} ${id} protected disk`);
    if(id.includes('_nut_')){
     const support=seat.left.part_id==='base'?seat.left:seat.right;
     assert.equal(support.part_content_hash,basePart.content_hash,
      `${phase} ${id} actual pocketed base seat`);
     assert.equal(support.semantic_datum.definition.role,'start_cap',
      `${phase} ${id} uses intact underside cap`);
    }
   }
   const gaps=Object.entries(clearance.checks).filter(([id])=>/^tight_slot_\d+_shank_gap$/.test(id));
   assert.equal(gaps.length,4,`${phase} four source-named shank gaps`);
   for(const [id,check] of gaps){
    assert.equal(check.status,'passed',`${phase} ${id}`);
    assert.equal(check.query.method,'slot_trim_witness');
    assert.equal(check.left.part_content_hash,basePart.content_hash,
     `${phase} ${id} bound to pocketed base`);
    assert.ok(check.lower_bound_mm>=.49,`${phase} ${id} certified native bound`);
   }
   const report=await page.goto(`${prefix}/nominal-seating-checks.html`,{waitUntil:'domcontentloaded'});
   assert.ok(report?.ok(),`${phase} seating report`);
   const reportText=await page.locator('body').innerText();
   for(const [id] of seats)assert.ok(reportText.includes(id),`${phase} visible seat ${id}`);
   for(const href of await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href))){
    assert.equal((await page.request.head(href)).status(),200,`${phase} report link ${href}`);
   }
   const preview=await page.goto(`${prefix}/drive_module_tightened.preview.html`,{waitUntil:'domcontentloaded'});
   assert.ok(preview?.ok(),`${phase} preview`);
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#occurrence-select option').count(),31,
    `${phase} 30 parts and selection prompt`);
   const selected=[];
   for(const [instance,part] of [['fixed_base','base'],
                                  ['tight_bolt_0','clamp_bolt_envelope'],
                                  ['tight_nut_0','nut_envelope']]){
    const option=await page.locator('#occurrence-select option').evaluateAll(
     (nodes,name)=>nodes.find(node=>node.textContent.includes(name))?.value,instance);
    assert.ok(option,`${phase} ${instance} selectable`);
    await page.locator('#occurrence-select').selectOption(option);
    const declaration=provenance.parts[part]?.declaration;
    assert.equal(declaration?.file,'drive_module_rotation.star',`${phase} ${part} loaded source`);
    await page.waitForFunction(span=>{
     const node=document.querySelector('#recipe-source');
     return node?.dataset.sourceFile===span.file&&
      node?.dataset.startByte===String(span.start_byte)&&
      node?.dataset.endByte===String(span.end_byte);
    },declaration);
    assert.equal(await page.locator('#recipe-module').inputValue(),declaration.file);
    assert.equal(await page.locator('#recipe-source').textContent(),textFor(bundle,declaration.file));
    assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,declaration));
    assert.ok((await page.locator('#recipe-summary').textContent()).includes(instance));
    const featureId=provenance.part_body_features[part];
    if(part==='base')assert.equal(featureId,'base_deep_relief');
    const feature=provenance.features[featureId]?.declaration;
    assert.equal(feature?.file,'drive_module_rotation.star');
    await page.locator('#recipe-feature-source').click();
    await page.waitForFunction(span=>
     document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),feature);
    assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,feature));
    assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
    selected.push({instance,part,feature:featureId});
   }
   const screenshot=`/tmp/cad-pocketed-mounting-accepted-${phase}.png`;
   await page.screenshot({path:screenshot});screenshots.push(screenshot);
   state[phase]={root:bundle.source,module:bundle.modules['drive_module_rotation.star'],
                 baseHash:basePart.content_hash,baseAssetSha:sha(baseAsset),
                 assemblyHash:assembly.content_hash,
                 bundleSha:sha(bundleBytes),manifestSha:sha(manifestBytes)};
   results.push({phase,endpoint,occurrences:assembly.instances,
                 seats:seats.length,gaps:gaps.length,selected});
  }
  assert.notEqual(state.before.root,state.after.root,'accepted endpoint source revisions differ');
  assert.equal(state.before.module,state.after.module,'same captured pocketed drive module');
  assert.equal(state.before.baseHash,state.after.baseHash,'same actual pocketed base across endpoints');
  assert.equal(state.before.baseAssetSha,state.after.baseAssetSha,
   'same exact native pocketed base bytes');
  assert.notEqual(state.before.assemblyHash,state.after.assemblyHash,'endpoints place common upper assembly differently');
  assert.equal(sha(await(await page.request.get(`${base}/evidence.json`)).body()),evidenceHash,
   'durable acceptance evidence unchanged during browser inspection');
  for(const phase of ['before','after']){
   for(const [name,expected] of [['recipe.bundle.json',state[phase].bundleSha],
                                 ['manifest.json',state[phase].manifestSha]]){
    assert.equal(sha(await(await page.request.get(`${base}/${phase}/${name}`)).body()),expected,
     `${phase} ${name} unchanged during browser inspection`);
   }
  }
  for(const screenshot of screenshots)assert.ok(fs.statSync(screenshot).size>10000,
   `captured screenshot ${screenshot}`);
  assert.deepEqual(errors,[],'no browser JavaScript errors');
  assert.deepEqual(writes,[],'demo viewer and reports are read-only');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'browser checker did not change');
  console.log(JSON.stringify({valid:true,read_only:true,states:results,
   evidence_sha256:evidenceHash,
   common_base_content_hash:state.before.baseHash,
   common_base_native_sha256:state.before.baseAssetSha,
   screenshots,browser_script_sha256:scriptSha},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
