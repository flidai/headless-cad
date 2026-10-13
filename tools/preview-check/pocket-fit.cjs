// Read-only browser acceptance for source-bound native capsule-pocket fit.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const root=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
const base=root+'/seated_pocket_inserts';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const sourceFor=(bundle,file)=>file===bundle.source_name?bundle.source:bundle.modules[file];
const spanText=(bundle,span)=>Buffer.from(sourceFor(bundle,span.file),'utf8')
 .subarray(span.start_byte,span.end_byte).toString('utf8');

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[];
 try{
  const page=await browser.newPage({viewport:{width:1800,height:1400},deviceScaleFactor:1});
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{if(!['GET','HEAD'].includes(request.method()))
   writes.push(`${request.method()} ${request.url()}`);});
  const get=async name=>{const response=await page.request.get(`${base}/${name}`);
   assert.equal(response.status(),200,name);return response;};
  const manifestBytes=await(await get('manifest.json')).body();
  const manifest=JSON.parse(manifestBytes.toString('utf8'));
  const reportBytes=await(await get('pocket-fit-checks.json')).body();
  const report=JSON.parse(reportBytes.toString('utf8'));
  const bundleBytes=await(await get('recipe.bundle.json')).body();
  const bundle=JSON.parse(bundleBytes.toString('utf8'));
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  assert.equal(manifest.recipe_source.name,'seated_pocket_inserts.star');
  assert.equal(bundle.source_name,manifest.recipe_source.name);
  assert.equal(report.kind,'native_capsule_pocket_fit');
  assert.equal(report.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.ok(bundle.modules['mixed_slot_fixture.star'],'actual imported pocket stock source captured');
  for(const [file,item] of Object.entries(provenance.source_inventory)){
   const source=sourceFor(bundle,file);assert.equal(typeof source,'string',file);
   const bytes=Buffer.from(source,'utf8');
   assert.equal(bytes.length,item.bytes,`${file} source bytes`);
   assert.equal(sha(bytes),item.sha256,`${file} captured source digest`);
  }
  assert.equal(manifest.outputs.seated_pocket_inserts.instances,3);
  assert.deepEqual(new Set(Object.keys(report.checks)),
   new Set(['first_insert_pocket_fit','second_insert_pocket_fit']));
  const values={};
  for(const name of ['first_insert','second_insert']){
   const id=name+'_pocket_fit',fit=report.checks[id];
   assert.deepEqual(fit,manifest.pocket_fit_checks[id]);
   assert.equal(fit.status,'passed');
   assert.equal(fit.declaration.output,'seated_pocket_inserts');
   assert.equal(fit.declaration.min_side_clearance_mm,.04);
   assert.equal(fit.assembly_content_hash,manifest.outputs.seated_pocket_inserts.content_hash);
   assert.equal(fit.pocket.part_id,'mixed_fixture_part');
   assert.equal(fit.insert.part_id,name);
   assert.equal(fit.pocket.part_content_hash,manifest.feature_hashes.relief_pocket);
   assert.equal(fit.insert.part_content_hash,manifest.feature_hashes[name+'_body']);
   assert.equal(fit.pocket.semantic_datum.selection.status,'resolved');
   assert.equal(fit.insert.semantic_datum.selection.status,'resolved');
   assert.equal(fit.pocket.semantic_datum.selection.witnesses[0].target_face,
    fit.query.pocket_floor_face);
   assert.equal(fit.insert.semantic_datum.selection.witnesses[0].target_face,
    fit.query.insert_floor_face);
   assert.equal(fit.query.pocket_wall_faces.length,4);
   assert.equal(new Set(fit.query.pocket_wall_faces).size,4);
   assert.ok(fit.query.pocket_wall_faces.includes(fit.query.pocket_witness_face));
   assert.ok(fit.side_clearance_lower_mm>=.04&&fit.side_clearance_lower_mm<.2&&
    fit.side_clearance_upper_mm>.2,'guarded interval contains exact .2 mm baseline margin');
   assert.ok(fit.axial_overlap_lower_mm>0);
   assert.ok(fit.signed_floor_gap_lower_mm<=0&&fit.signed_floor_gap_upper_mm>=0);
   assert.equal(manifest.nominal_seating_checks[name+'_floor_seat'].status,'passed');
   assert.ok(Math.abs(manifest.datum_dimensions[name+'_proud_height'].value_mm-2)<1e-7);
   values[id]=[fit.side_clearance_lower_mm,fit.side_clearance_upper_mm];
  }
  const fitPage=await page.goto(`${base}/pocket-fit-checks.html`,{waitUntil:'load'});
  assert.ok(fitPage?.ok(),'new native fit report page loads');
  assert.equal(await page.locator('table tbody tr').count(),2);
  const rows=await page.locator('table tbody tr').allTextContents();
  for(const id of Object.keys(report.checks))assert.ok(rows.some(row=>row.includes(id)&&
   row.includes('passed')&&row.includes('0.04')),`${id} visible native interval row`);
  assert.ok((await page.locator('body').textContent()).includes('Floor seating is checked separately'));
  const reportShot='/tmp/cad-pocket-fit-report.png';
  await page.screenshot({path:reportShot,fullPage:true});

  const catalog=await page.request.get(root+'/');
  assert.equal(catalog.status(),200,'development catalog loads');
  const catalogHtml=await catalog.text();
  assert.ok(catalogHtml.includes('Pocket fit')&&
   catalogHtml.includes('seated_pocket_inserts/pocket-fit-checks.html'),
   'server catalog links exact new fit report');
  const preview=await page.goto(`${base}/seated_pocket_inserts.gpu.html`,
   {waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'actual assembly GPU viewer loads');
  await page.locator('#recipe-toggle').click();
  await page.locator('#occurrence-index').fill('0');
  await page.locator('#inspect-index').click();
  const partSpan=provenance.parts.first_insert.declaration;
  await page.waitForFunction(span=>{
   const source=document.querySelector('#recipe-source');
   return source?.dataset.sourceFile===span.file&&
    source?.dataset.startByte===String(span.start_byte);
  },partSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),
   spanText(bundle,partSpan));
  assert.ok((await page.locator('#selection').textContent()).includes('first_insert'));
  const featureSpan=provenance.features.first_insert_body.declaration;
  await page.locator('#recipe-feature-source').click();
  await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===
   String(span.start_byte),featureSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),
   spanText(bundle,featureSpan));
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
  const viewerShot='/tmp/cad-pocket-fit-viewer.png';
  await page.screenshot({path:viewerShot});
  assert.equal(sha(await(await get('manifest.json')).body()),sha(manifestBytes));
  assert.equal(sha(await(await get('pocket-fit-checks.json')).body()),sha(reportBytes));
  assert.equal(sha(await(await get('recipe.bundle.json')).body()),sha(bundleBytes));
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha);
  assert.deepEqual(errors,[],'zero browser errors');
  assert.deepEqual(writes,[],'browser issues no writes');
  console.log(JSON.stringify({valid:true,instances:3,fit_checks:2,floor_seats:2,
   native_height_dimensions:2,source_navigation:true,read_only:true,
   native_intervals_mm:values,manifest_sha256:sha(manifestBytes),
   pocket_fit_report_sha256:sha(reportBytes),bundle_sha256:sha(bundleBytes),
   source_provenance_sha256:sha(provenanceBytes),script_sha256:scriptSha,
   screenshots:[reportShot,viewerShot]},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
