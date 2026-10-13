// Read-only browser acceptance of the two source-owned native pocket-floor inserts.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/seated_pocket_inserts';
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
  const bundleBytes=await(await get('recipe.bundle.json')).body();
  const bundle=JSON.parse(bundleBytes.toString('utf8'));
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const seatReport=await(await get('nominal-seating-checks.json')).json();
  const dimensionReport=await(await get('datum-dimensions.json')).json();
  const sheet=await(await get('drawing-seated_insert_heights.json')).json();
  assert.equal(manifest.recipe_source.name,'seated_pocket_inserts.star');
  assert.equal(bundle.source_name,manifest.recipe_source.name);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(seatReport.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(dimensionReport.recipe_context_hash,manifest.recipe_context_hash);
  for(const [file,item] of Object.entries(provenance.source_inventory)){
   const source=sourceFor(bundle,file);assert.equal(typeof source,'string',file);
   const bytes=Buffer.from(source,'utf8');
   assert.equal(bytes.length,item.bytes,`${file} exact captured bytes`);
   assert.equal(sha(bytes),item.sha256,`${file} exact source digest`);
  }
  assert.ok(bundle.modules['mixed_slot_fixture.star'],'actual imported fixture source is captured');
  assert.equal(manifest.outputs.seated_pocket_inserts.instances,3);
  for(const [name,role,depth] of [['first_insert','slot_locating_floor',3],
                                   ['second_insert','slot_relief_floor',7]]){
   const seat=manifest.nominal_seating_checks[`${name}_floor_seat`];
   assert.deepEqual(seat,seatReport.checks[`${name}_floor_seat`]);
   assert.equal(seat.status,'passed');
   assert.ok(seat.disk_radius_lower_mm>=1);
   assert.ok(seat.signed_gap_lower_mm<=0&&seat.signed_gap_upper_mm>=0);
   assert.equal(seat.left.part_id,name);
   assert.equal(seat.right.part_id,'mixed_fixture_part');
   assert.equal(seat.right.semantic_datum.definition.role,role);
   assert.equal(seat.right.semantic_datum.selection.status,'resolved');
   assert.equal(seat.right.semantic_datum.selection.witnesses.length,1);
   assert.ok(Math.abs(seat.query.disk_center[2]-(12-depth))<1e-7);
   const dimension=manifest.datum_dimensions[`${name}_proud_height`];
   assert.deepEqual(dimension,dimensionReport.dimensions[`${name}_proud_height`]);
   assert.equal(dimension.status,'passed');
   assert.ok(Math.abs(dimension.value_mm-2)<1e-7);
   assert.equal(dimension.from.part_id,'mixed_fixture_part');
   assert.equal(dimension.to.part_id,name);
  }
  const firstFloor=manifest.face_assertions.first_floor_survives.witnesses[0];
  assert.notEqual(firstFloor.anchor_faces[0],firstFloor.target_face);
  assert.equal(manifest.nominal_seating_checks.first_insert_floor_seat.right
   .semantic_datum.selection.witnesses[0].target_face,firstFloor.target_face);
  const section=await(await get('section-insert_tops.json')).json();
  assert.equal(section.section_view.status,'passed');
  const sections=Object.fromEntries(section.occurrences.map(item=>[item.instance_id,item.section]));
  assert.equal(sections.first_insert.loops.length,1,'closed first insert contour');
  assert.equal(sections.second_insert.loops.length,1,'closed second insert contour');
  assert.equal(sections.fixture.loops.length,0,'no plate at protruding section');
  const area=(length,radius)=>2*radius*length+Math.PI*radius*radius;
  assert.ok(Math.abs(section.section.area-(area(20,4.8)+area(22,5.8)))<1e-6);

  const drawing=await page.goto(`${base}/drawing-seated_insert_heights.svg`,{waitUntil:'load'});
  assert.ok(drawing?.ok(),'native insert height drawing loads');
  const visual=await page.evaluate(()=>{
   const root=document.documentElement;
   const box=element=>{const b=element.getBoundingClientRect();return {left:b.left,top:b.top,
    right:b.right,bottom:b.bottom,width:b.width,height:b.height};};
   const view=root.querySelector('#drawing-view-insert_tops');
   const table=root.querySelector('#drawing-table-insert_heights');
   return {paper:{width:root.getAttribute('width'),height:root.getAttribute('height'),
     viewBox:root.getAttribute('viewBox')},
    view:view&&{box:box(view),paths:[...view.querySelectorAll('path[id]')].map(p=>p.getAttribute('d')),
     font:Number(view.dataset.paperAnnotationFontMm)},
    table:table&&{box:box(table),rows:[...table.querySelectorAll('g[data-dimension]')].map(row=>({
     id:row.dataset.dimension,text:row.textContent.replace(/\s+/g,' ').trim(),box:box(row),
     labels:[...row.querySelectorAll('text')].map(label=>box(label))}))}};
  });
  assert.deepEqual(visual.paper,{width:'420mm',height:'297mm',viewBox:'0 0 420 297'});
  assert.ok(visual.view&&visual.view.paths.length===3&&visual.view.font===2.8,
   'native two-loop section drawn at authored paper font');
  assert.equal(visual.view.paths.filter(path=>path.includes('A ')).length,2,
   'both real capsule arcs render; plate has no section at protruding level');
  assert.ok(visual.table&&visual.table.rows.length===2);
  assert.deepEqual(new Set(visual.table.rows.map(row=>row.id)),
   new Set(['first_insert_proud_height','second_insert_proud_height']));
  for(const row of visual.table.rows){
   assert.ok(row.text.includes('≈ 2.000 mm'),`${row.id} actual native value visible`);
   for(const label of row.labels)assert.ok(label.left>=visual.table.box.left-1&&
    label.right<=visual.table.box.right+1&&label.top>=visual.table.box.top-1&&
    label.bottom<=visual.table.box.bottom+1,`${row.id} visible inside paper table`);
  }
  const drawingShot='/tmp/cad-pocketed-inserts-drawing.png';
  const drawingCdp=await page.context().newCDPSession(page);
  fs.writeFileSync(drawingShot,Buffer.from((await drawingCdp.send('Page.captureScreenshot',
   {format:'png',captureBeyondViewport:true})).data,'base64'));
  await drawingCdp.detach();

  const preview=await page.goto(`${base}/seated_pocket_inserts.gpu.html`,
   {waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'actual three-leaf GPU preview loads');
  await page.locator('#recipe-toggle').click();
  for(const [index,partId,featureId] of [[0,'first_insert','first_insert_body'],
                                          [1,'mixed_fixture_part','relief_pocket'],
                                          [2,'second_insert','second_insert_body']]){
   await page.locator('#occurrence-index').fill(String(index));
   await page.locator('#inspect-index').click();
   const partSpan=provenance.parts[partId].declaration;
   await page.waitForFunction(span=>{
    const source=document.querySelector('#recipe-source');
    return source?.dataset.sourceFile===span.file&&
     source?.dataset.startByte===String(span.start_byte);
   },partSpan);
   assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,partSpan));
   assert.ok((await page.locator('#selection').textContent()).includes(partId));
   const featureSpan=provenance.features[featureId].declaration;
   await page.locator('#recipe-feature-source').click();
   await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===
    String(span.start_byte),featureSpan);
   assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,featureSpan));
   assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
  }
  const assemblyShot='/tmp/cad-pocketed-inserts-assembly.png';
  await page.screenshot({path:assemblyShot});
  const manifestEndBytes=await(await get('manifest.json')).body();
  assert.equal(sha(manifestEndBytes),sha(manifestBytes),'browser stayed on one publication');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'browser checker stayed pinned');
  assert.deepEqual(errors,[],'zero browser errors');
  assert.deepEqual(writes,[],'no browser writes');
  console.log(JSON.stringify({valid:true,instances:3,native_floor_seats:2,
   native_protrusion_dimensions:2,section_loops:2,source_navigation:true,read_only:true,
   manifest_sha256:sha(manifestBytes),manifest_end_sha256:sha(manifestEndBytes),
   source_provenance_sha256:sha(provenanceBytes),bundle_sha256:sha(bundleBytes),
   script_sha256:scriptSha,screenshots:[drawingShot,assemblyShot]},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
