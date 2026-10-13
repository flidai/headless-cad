// Browser acceptance of the source-owned serial blind-slot fixture and machining sheet.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/serial_blind_fixture';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const sourceFor=(bundle,label)=>label===bundle.source_name?bundle.source:bundle.modules[label];
const spanText=(bundle,span)=>Buffer.from(sourceFor(bundle,span.file),'utf8')
 .subarray(span.start_byte,span.end_byte).toString('utf8');
const expectedDimensions={locating_depth:3,relief_depth:7,remaining_web:5};

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[];
 try{
  const page=await browser.newPage({viewport:{width:1800,height:1400},deviceScaleFactor:1});
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(!['GET','HEAD'].includes(r.method()))writes.push(`${r.method()} ${r.url()}`);});
  const get=async name=>{const response=await page.request.get(`${base}/${name}`);
   assert.equal(response.status(),200,name);return response;};
  const manifestBytes=await(await get('manifest.json')).body();
  const manifest=JSON.parse(manifestBytes.toString('utf8'));
  const source=await(await get('recipe.star')).text();
  const bundle={source_name:'serial_blind_fixture.star',source,modules:{}};
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const sheet=await(await get('drawing-serial_fixture_machining.json')).json();
  const datum=await(await get('datum-dimensions.json')).json();
  assert.equal(manifest.recipe_source.name,'serial_blind_fixture.star');
  assert.equal(bundle.source_name,manifest.recipe_source.name);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.source_bundle_hash,null,'single-file recipe has no module bundle');
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(datum.recipe_context_hash,manifest.recipe_context_hash);
  for(const [label,item] of Object.entries(provenance.source_inventory)){
   const source=sourceFor(bundle,label);assert.equal(typeof source,'string',label);
   const bytes=Buffer.from(source,'utf8');
   assert.equal(bytes.length,item.bytes,`${label} source bytes`);
   assert.equal(sha(bytes),item.sha256,`${label} captured source digest`);
  }
  const sections={};
  for(const name of ['both_pockets','deep_pocket_only','intact_web']){
   const report=await(await get(`section-${name}.json`)).json();
   assert.equal(report.snapshot.recipe_source_hash,manifest.recipe_source_hash);
   assert.equal(report.section_view.status,'passed');
   assert.equal(report.section_view.declaration.id,name);
   sections[name]=report;
  }
  const stockArea=120*80-4*Math.PI*3*3;
  const locatingArea=2*5*20+Math.PI*5*5;
  const reliefArea=2*6*22+Math.PI*6*6;
  assert.ok(Math.abs(sections.both_pockets.section.area-
   (stockArea-locatingArea-reliefArea))<1e-6,'opening section shows both actual capsule voids');
  assert.ok(Math.abs(sections.deep_pocket_only.section.area-
   (stockArea-reliefArea))<1e-6,'middle section retains only deeper pocket');
  assert.ok(Math.abs(sections.intact_web.section.area-stockArea)<1e-6,
   'web section has no capsule voids');
  const floor=manifest.face_assertions.first_floor_survives;
  const walls=manifest.face_assertions.first_walls_survive;
  assert.equal(floor.status,'resolved');assert.equal(walls.status,'resolved');
  assert.equal(floor.selector.anchor_feature,'locating_pocket');
  assert.equal(floor.selector.target_feature,'relief_pocket');
  assert.equal(floor.selector.role,'slot_locating_floor');
  assert.equal(floor.witnesses.length,1);
  assert.equal(floor.witnesses[0].anchor_faces.length,1);
  assert.equal(floor.witnesses[0].target_face,floor.witnesses[0].anchor_faces[0]+4,
   'first floor face remaps after second wall insertion');
  assert.ok(floor.lineage.length>0,'first floor has explicit operation ancestry');
  assert.equal(walls.witnesses.length,4);
  for(const witness of walls.witnesses){
   assert.ok(witness.anchor_faces.includes(witness.target_face),'first pocket walls retain IDs');
  }
  for(const [id,value] of Object.entries(expectedDimensions)){
   const measured=datum.dimensions[id];assert.ok(measured,`${id} native report`);
   assert.equal(measured.status,'passed');
   assert.equal(measured.declaration.output,'serial_fixture_layout');
   assert.ok(Math.abs(measured.value_mm-value)<1e-8,`${id} native value`);
   assert.deepEqual(measured,manifest.datum_dimensions[id]);
   const floorName=id==='locating_depth'?'first_floor':id==='relief_depth'?'second_floor':null;
   if(floorName){
    assert.equal(measured.to.datum,floorName);
    assert.equal(measured.to.semantic_datum.selection.witnesses.length,1);
    if(floorName==='first_floor'){
     assert.equal(measured.to.semantic_datum.definition.anchor_feature,'locating_pocket');
     assert.equal(measured.to.semantic_datum.selection.witnesses[0].target_face,
      floor.witnesses[0].target_face);
    }
   }
   for(const endpoint of [measured.from,measured.to]){
    assert.deepEqual(endpoint.instance_path,['fixture_leaf']);
    assert.equal(endpoint.part_id,'serial_fixture_part');
    assert.equal(endpoint.semantic_datum.selection.status,'resolved');
    assert.equal(endpoint.semantic_datum.selection.witnesses.length,1);
    assert.equal(endpoint.semantic_datum.target_content_hash,endpoint.part_content_hash);
   }
  }
  const drawing=await page.goto(`${base}/drawing-serial_fixture_machining.svg`,{waitUntil:'load'});
  assert.ok(drawing?.ok(),'native blind fixture drawing loads');
  const visual=await page.evaluate(()=>{
   const root=document.documentElement;
   const box=e=>{const b=e.getBoundingClientRect();return {left:b.left,top:b.top,right:b.right,bottom:b.bottom,width:b.width,height:b.height};};
   const paper={width:root.getAttribute('width'),height:root.getAttribute('height'),
    viewBox:root.getAttribute('viewBox'),box:box(root)};
   const views=[...root.querySelectorAll('svg[id^="drawing-view-"]')].map(view=>({
    id:view.id,scale:Number(view.dataset.scale),font:Number(view.dataset.paperAnnotationFontMm),
    box:box(view),paths:[...view.querySelectorAll('path[id]')].map(path=>path.getAttribute('d')),
    caption:(()=>{const c=root.querySelector(`#drawing-caption-${view.id.slice('drawing-view-'.length)}`);
     return c&&{text:c.textContent,font:Number(c.getAttribute('font-size')),box:box(c)};})()}));
   const tables=[...root.querySelectorAll('g[id^="drawing-table-"]')].filter(g=>!g.hasAttribute('data-dimension')).map(g=>({
    id:g.id,box:box(g),rows:[...g.querySelectorAll('g[data-dimension]')].map(row=>({
     id:row.id,dimension:row.dataset.dimension,text:row.textContent.replace(/\s+/g,' ').trim(),box:box(row),
     labels:[...row.querySelectorAll('text')].map(t=>({text:t.textContent,box:box(t)}))}))}));
   return {paper,views,tables,footer:[...root.querySelectorAll('text')].map(t=>t.textContent).filter(Boolean)};
  });
  assert.equal(visual.paper.width,'420mm');assert.equal(visual.paper.height,'297mm');
  assert.equal(visual.paper.viewBox,'0 0 420 297');
  assert.deepEqual(new Set(visual.views.map(v=>v.id)),new Set([
   'drawing-view-both_pockets','drawing-view-deep_pocket_only','drawing-view-intact_web']));
  const inside=visual.views.find(v=>v.id==='drawing-view-both_pockets');
  const middle=visual.views.find(v=>v.id==='drawing-view-deep_pocket_only');
  const web=visual.views.find(v=>v.id==='drawing-view-intact_web');
  assert.ok(inside.paths.some(path=>path.includes('A ')),'inside section renders native capsule arcs');
  assert.ok(middle.paths.some(path=>path.includes('A ')),'middle section renders deeper capsule arcs');
  assert.ok(web.paths.length>0,'intact web section renders geometry');
  for(const view of visual.views){
   assert.equal(view.scale,.8);assert.equal(view.font,2.8);
   assert.equal(view.caption.font,2.8);
   const reportView=sheet.views.find(v=>`drawing-view-${v.viewport.view}`===view.id);
   assert.ok(reportView,`${view.id} source-authored viewport`);
   assert.equal(view.caption.text,reportView.viewport.paper_caption);
   const pixelsPerMm=visual.paper.box.width/420;
   assert.ok(view.caption.box.left>=visual.paper.box.left+reportView.viewport.x*pixelsPerMm-2&&
    view.caption.box.right<=visual.paper.box.left+
     (reportView.viewport.x+reportView.viewport.width)*pixelsPerMm+2,
    `${view.id} caption inside authored paper band`);
   assert.ok(view.box.left>=visual.paper.box.left-1&&view.box.right<=visual.paper.box.right+1&&
    view.box.top>=visual.paper.box.top-1&&view.box.bottom<=visual.paper.box.bottom+1,
    `${view.id} viewport inside paper`);
  }
  assert.equal(visual.tables.length,1);
  const table=visual.tables[0],reportTable=sheet.dimension_tables[0];
  assert.equal(table.id,'drawing-table-depths');
  assert.equal(reportTable.id,'depths');
  assert.equal(reportTable.output,'serial_fixture_layout');
  assert.equal(table.rows.length,3);assert.equal(reportTable.rows.length,3);
  assert.deepEqual(new Set(table.rows.map(r=>r.dimension)),new Set(Object.keys(expectedDimensions)));
  for(const reportRow of reportTable.rows){
   const row=table.rows.find(r=>r.dimension===reportRow.dimension);
   assert.ok(row,`${reportRow.dimension} visible`);
   assert.equal(row.id,`drawing-table-depths-${reportRow.dimension}`);
   assert.deepEqual(reportRow.native_dimension,datum.dimensions[reportRow.dimension]);
   assert.ok(row.text.includes(reportRow.label),`${reportRow.dimension} authored label`);
   assert.ok(row.text.includes(`≈ ${expectedDimensions[reportRow.dimension].toFixed(3)} mm`),
    `${reportRow.dimension} visible native value`);
   for(const label of row.labels){
    assert.ok(label.box.left>=table.box.left-1&&label.box.right<=table.box.right+1&&
     label.box.top>=table.box.top-1&&label.box.bottom<=table.box.bottom+1,
     `${reportRow.dimension} text inside table`);
   }
  }
  for(let i=0;i<table.rows.length;i++)for(let j=i+1;j<table.rows.length;j++){
   const a=table.rows[i].box,b=table.rows[j].box;
   assert.equal(a.left<b.right&&b.left<a.right&&a.top<b.bottom&&b.top<a.bottom,false,
    `${table.rows[i].dimension}/${table.rows[j].dimension} visible rows separate`);
  }
  assert.ok(visual.footer.some(text=>text.includes(`Source context: ${manifest.recipe_context_hash}`)));
  const cdp=await page.context().newCDPSession(page);
  const drawingShot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});
  fs.writeFileSync('/tmp/cad-serial-blind-slots-drawing.png',Buffer.from(drawingShot.data,'base64'));
  await cdp.detach();

  const preview=await page.goto(`${base}/serial_fixture_layout.gpu.html`,{waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'assembly GPU preview loads');
  await page.locator('#recipe-toggle').click();
  await page.locator('#occurrence-index').fill('0');
  await page.locator('#inspect-index').click();
  const partSpan=provenance.parts.serial_fixture_part.declaration;
  assert.equal(partSpan.file,bundle.source_name);
  await page.waitForFunction(span=>{
   const source=document.querySelector('#recipe-source');
   return source?.dataset.sourceFile===span.file&&source?.dataset.startByte===String(span.start_byte);
  },partSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,partSpan));
  assert.ok((await page.locator('#selection').textContent()).includes('serial_fixture_part'));
  const featureSpan=provenance.features.relief_pocket.declaration;
  await page.locator('#recipe-feature-source').click();
  await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),featureSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,featureSpan));
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
  const body=await page.goto(`${base}/fixture.preview.html`,{waitUntil:'domcontentloaded'});
  assert.ok(body?.ok(),'default part preview loads');
  await page.locator('canvas').first().waitFor({state:'visible'});
  const bodyCdp=await page.context().newCDPSession(page);
  const bodyShot=await bodyCdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  fs.writeFileSync('/tmp/cad-serial-blind-slots-body.png',Buffer.from(bodyShot.data,'base64'));
  await bodyCdp.detach();
  const manifestEndBytes=await(await get('manifest.json')).body();
  assert.equal(sha(manifestEndBytes),sha(manifestBytes),'browser stayed on one manifest revision');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'browser script stayed pinned');
  assert.deepEqual(errors,[],'zero browser errors');
  assert.deepEqual(writes,[],'no browser write requests');
  console.log(JSON.stringify({valid:true,pockets:2,through_bores:4,sections:3,native_depth_rows:3,
   source_navigation:true,read_only:true,manifest_sha256:sha(manifestBytes),
   manifest_end_sha256:sha(manifestEndBytes),script_sha256:scriptSha,
   recipe_context_hash:manifest.recipe_context_hash,source_provenance_sha256:sha(provenanceBytes),
   screenshots:['/tmp/cad-serial-blind-slots-drawing.png','/tmp/cad-serial-blind-slots-body.png']},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
