// Read-only browser acceptance for the source-bound motor housing profile pockets.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const root=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
const base=(process.env.CAD_ARTIFACT_BASE||root+'/motor_mount_housing').replace(/\/$/,'');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const spanText=(source,span)=>Buffer.from(source,'utf8')
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
  const sourceBytes=await(await get('recipe.star')).body();
  const source=sourceBytes.toString('utf8');
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const dimensionsBytes=await(await get('datum-dimensions.json')).body();
  const dimensions=JSON.parse(dimensionsBytes.toString('utf8'));
  const drawingBytes=await(await get('drawing-motor_housing_machining.svg')).body();
  const drawing=drawingBytes.toString('utf8');
  const assemblyBytes=await(await get('motor_mount_housing.assembly.json')).body();
  const assembly=JSON.parse(assemblyBytes.toString('utf8'));
  assert.equal(manifest.recipe_source.name,'motor_mount_housing.star');
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(dimensions.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.source_inventory['motor_mount_housing.star'].sha256,sha(sourceBytes));
  assert.equal(provenance.source_inventory['motor_mount_housing.star'].bytes,sourceBytes.length);
  assert.equal(manifest.outputs.motor_mount_housing.instances,2);
  assert.deepEqual(new Set(assembly.instances.map(instance=>instance.id)),
   new Set(['housing','cover']));
  const housingIndex=assembly.instances.findIndex(instance=>instance.id==='housing');
  assert.ok(housingIndex>=0);
  assert.equal(manifest.outputs.housing.feature_id,'cable_pocket');
  assert.equal(manifest.outputs.first_stage.feature_id,'motor_recess');
  assert.equal(manifest.outputs.cover.feature_id,'cover_body');
  for(const id of ['motor_depth','cable_depth','remaining_web']){
   const report=dimensions.dimensions[id];
   assert.deepEqual(report,manifest.datum_dimensions[id]);
   assert.equal(report.status,'passed');
   assert.deepEqual(report.from.instance_path,['housing']);
   assert.deepEqual(report.to.instance_path,['housing']);
   assert.equal(report.from.part_content_hash,manifest.outputs.housing.content_hash);
   assert.equal(report.to.part_content_hash,manifest.outputs.housing.content_hash);
   assert.ok(drawing.includes(`data-dimension="${id}"`),`${id} native drawing cross-reference`);
  }
  for(const [id,value] of [['motor_depth',12],['cable_depth',18],['remaining_web',6]]){
   assert.ok(Math.abs(dimensions.dimensions[id].value_mm-value)<1e-7,`${id} exact floor datum`);
   assert.ok(drawing.includes(`${value.toFixed(3)} mm`),`${id} visible drawing value`);
  }
  assert.deepEqual(new Set(Object.keys(manifest.section_views)),
   new Set(['both_cavities','cable_only','remaining_stock']));
  for(const name of ['both_cavities','cable_only','remaining_stock']){
   const response=await get(`section-${name}.json`),report=await response.json();
   assert.equal(report.section.area,manifest.section_views[name].query.section.area);
   assert.equal(report.section.loops.length,
    {both_cavities:9,cable_only:8,remaining_stock:7}[name]);
  }
  assert.equal(manifest.drawing_sheets.motor_housing_machining.status,'published');
  assert.deepEqual(new Set(manifest.drawing_sheets.motor_housing_machining.datum_dimension_ids),
   new Set(['motor_depth','cable_depth','remaining_web']));
  assert.equal((drawing.match(/data-dimension="/g)||[]).length,3);
  assert.ok(drawing.includes('Source context: '+manifest.recipe_context_hash));
  assert.deepEqual(new Set(Object.keys(manifest.property_assertions)),
   new Set(['machined_volume','layout_material_mass']));
  for(const [id,property,target] of [
   ['machined_volume','volume_mm3',manifest.outputs.housing.content_hash],
   ['layout_material_mass','mass_kg',manifest.outputs.motor_mount_housing.content_hash]]){
   const check=manifest.property_assertions[id];
   assert.equal(check.status,'passed');
   assert.equal(check.property,property);
   assert.equal(check.target_content_hash,target);
   assert.ok(check.actual>=check.min&&check.actual<=check.max);
  }
  const dimensionPage=await page.goto(`${base}/datum-dimensions.html`,{waitUntil:'load'});
  assert.ok(dimensionPage?.ok(),'native dimension report loads');
  const dimensionText=await page.locator('body').textContent();
  for(const id of ['motor_depth','cable_depth','remaining_web'])
   assert.ok(dimensionText.includes(id),`${id} visible in native report`);
  const dimensionShot='/tmp/cad-profile-pocket-dimensions.png';
  await page.screenshot({path:dimensionShot,fullPage:true});
  const drawingPage=await page.goto(`${base}/drawing-motor_housing_machining.svg`,{waitUntil:'load'});
  assert.ok(drawingPage?.ok(),'three-section native drawing loads');
  const drawingShot='/tmp/cad-profile-pocket-drawing.png';
  const drawingCdp=await page.context().newCDPSession(page);
  const drawingCapture=await drawingCdp.send('Page.captureScreenshot',
   {format:'png',captureBeyondViewport:true});
  fs.writeFileSync(drawingShot,Buffer.from(drawingCapture.data,'base64'));
  await drawingCdp.detach();
  if(!process.env.CAD_ARTIFACT_BASE){
   const catalog=await page.request.get(root+'/');
   assert.equal(catalog.status(),200,'development catalog loads');
   assert.ok((await catalog.text()).includes('motor_mount_housing/'),
    'development catalog exposes motor housing gallery');
  }
  const preview=await page.goto(`${base}/motor_mount_housing.gpu.html`,
   {waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'actual two-leaf assembly GPU viewer loads');
  await page.waitForFunction(()=>document.querySelectorAll('.engineering-check').length===2);
  assert.equal(await page.locator('.engineering-check').count(),2);
  assert.deepEqual(new Set(await page.locator('.engineering-check').evaluateAll(
   nodes=>nodes.map(node=>node.dataset.assertionId))),
   new Set(['machined_volume','layout_material_mass']));
  await page.locator('#recipe-toggle').click();
  await page.locator('#occurrence-index').fill(String(housingIndex));
  await page.locator('#inspect-index').click();
  const partSpan=provenance.parts.housing_part.declaration;
  await page.waitForFunction(span=>{
   const element=document.querySelector('#recipe-source');
   return element?.dataset.sourceFile===span.file&&
    element?.dataset.startByte===String(span.start_byte);
  },partSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),
   spanText(source,partSpan));
  assert.ok((await page.locator('#selection').textContent()).includes('housing'));
  const featureSpan=provenance.features.cable_pocket.declaration;
  await page.locator('#recipe-feature-source').click();
  await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===
   String(span.start_byte),featureSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),
   spanText(source,featureSpan));
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
  const viewerShot='/tmp/cad-profile-pocket-viewer.png';
  await page.screenshot({path:viewerShot});
  assert.equal(sha(await(await get('manifest.json')).body()),sha(manifestBytes));
  assert.equal(sha(await(await get('recipe.star')).body()),sha(sourceBytes));
  assert.equal(sha(await(await get('source-provenance.json')).body()),sha(provenanceBytes));
  assert.equal(sha(await(await get('datum-dimensions.json')).body()),sha(dimensionsBytes));
  assert.equal(sha(await(await get('drawing-motor_housing_machining.svg')).body()),sha(drawingBytes));
  assert.equal(sha(await(await get('motor_mount_housing.assembly.json')).body()),sha(assemblyBytes));
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha);
  assert.deepEqual(errors,[],'zero browser errors');
  assert.deepEqual(writes,[],'browser issues no writes');
  console.log(JSON.stringify({valid:true,instances:2,native_dimensions:3,section_strata:3,
   property_checks:2,drawing_rows:3,source_navigation:true,read_only:true,
   manifest_sha256:sha(manifestBytes),source_sha256:sha(sourceBytes),
   assembly_sha256:sha(assemblyBytes),
   source_provenance_sha256:sha(provenanceBytes),
   datum_dimensions_sha256:sha(dimensionsBytes),drawing_sha256:sha(drawingBytes),
   script_sha256:scriptSha,
   screenshots:[dimensionShot,drawingShot,viewerShot]},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
