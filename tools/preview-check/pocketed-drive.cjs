// Inspect the real source-owned drive variant; never invoke model authoring from the viewer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');
const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_pocketed';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const expected={base_shallow_depth:1,base_deep_depth:2,base_remaining_web:4};
(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1800,height:1400}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(!['GET','HEAD'].includes(r.method()))writes.push(`${r.method()} ${r.url()}`);});
  const captured={};
  const bytes=async name=>{const r=await page.request.get(`${base}/${name}`);assert.equal(r.status(),200,name);
   const b=await r.body();captured[name]=sha(b);return b;};
  const json=async name=>JSON.parse((await bytes(name)).toString('utf8'));
  const manifest=await json('manifest.json');
  const source=(await bytes('recipe.star')).toString('utf8');
  const provenance=await json('source-provenance.json');
  assert.equal(manifest.recipe_source.name,'drive_module_rotation.star');
  assert.equal(manifest.recipe_source.parameter_overrides.base_pocket_depth,1);
  assert.match(source,/base_pocket_depth\s*=\s*param\([^\n]*default\s*=\s*0/);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  for(const [name,item] of Object.entries(provenance.source_inventory)){
   assert.equal(name,'drive_module_rotation.star');
   assert.equal(item.sha256,sha(Buffer.from(source)));assert.equal(item.bytes,Buffer.byteLength(source));
  }
  const assembly=await json('drive_module_release.assembly.json');
  assert.equal(assembly.instances.length,30);
  assert.equal(Object.keys(assembly.definitions).length,14);
  const baseIndex=assembly.instances.findIndex(i=>i.id===JSON.stringify(['base']));
  assert.ok(baseIndex>=0,'assembled base is a real placed occurrence');
  const groups=await json('rigid-rotation-groups.json');
  assert.equal(groups.groups.rotor.members.length,9);
  assert.equal(groups.groups.rotor.coverage,'named_pairs_only');
  assert.equal(groups.groups.rotor.clearance_check_ids.length,13);
  assert.equal(Object.keys(manifest.datum_dimensions).length,29);
  const floor=manifest.face_assertions.base_shallow_floor_survives;
  assert.equal(floor.status,'resolved');
  assert.equal(floor.selector.anchor_feature,'base_shallow_relief');
  assert.equal(floor.selector.target_feature,'base_deep_relief');
  assert.equal(floor.selector.role,'slot_shallow_floor');
  assert.equal(floor.witnesses.length,1);
  assert.equal(floor.witnesses[0].target_face,floor.witnesses[0].anchor_faces[0]+4);
  assert.ok(floor.lineage.some(l=>l.feature==='base_deep_relief'&&l.output_faces.includes(floor.witnesses[0].target_face)));
  for(const name of ['front_minus','front_plus','rear_minus','rear_plus']){
   const walls=manifest.face_assertions[`base_${name}_survives`];
   assert.equal(walls.status,'resolved');assert.equal(walls.witnesses.length,4);
   assert.equal(walls.selector.target_feature,'base_deep_relief');
   for(const w of walls.witnesses)assert.ok(w.anchor_faces.includes(w.target_face));
  }
  const datum=await json('datum-dimensions.json');
  for(const [id,value] of Object.entries(expected)){
   const d=datum.dimensions[id];assert.equal(d.status,'passed');
   assert.equal(d.declaration.output,'drive_module_release');
   assert.ok(Math.abs(d.value_mm-value)<1e-8,id);
   assert.deepEqual(d,manifest.datum_dimensions[id]);
   if(id==='base_shallow_depth'){
    const endpoint=[d.from,d.to].find(e=>e.datum==='shallow_floor');
    assert.ok(endpoint,'depth uses actual surviving shallow floor');
    assert.equal(endpoint.semantic_datum.selection.witnesses[0].target_face,floor.witnesses[0].target_face);
   }

   for(const e of [d.from,d.to]){
    assert.deepEqual(e.instance_path,['base']);assert.equal(e.part_id,'base');
    assert.equal(e.semantic_datum.selection.status,'resolved');
    assert.equal(e.semantic_datum.selection.witnesses.length,1);
    assert.equal(e.semantic_datum.target_content_hash,e.part_content_hash);
   }
  }
  const sections={};
  for(const name of ['base_relief_section','base_web_section']){
   const r=await json(`section-${name}.json`);
   assert.equal(r.snapshot.recipe_source_hash,manifest.recipe_source_hash);
   assert.equal(r.section_view.status,'passed');
   // Both body-output and explicitly placed section representations preserve native loops.
   const native=r.section||r.occurrences?.find(o=>JSON.stringify(o.instance_path)===JSON.stringify(['base']))?.section;
   assert.ok(native,`${name} actual base section`);sections[name]=native;
  }
  const capsules=s=>s.loops.filter(l=>l.segments.length===4&&l.segments.filter(e=>e.curve.kind==='arc').length===2);
  assert.equal(capsules(sections.base_relief_section).length,6,'top cuts four mounting slots and two blind pockets');
  assert.equal(capsules(sections.base_web_section).length,4,'web retains only four through slots');
  const pocketArea=108+9*Math.PI;
  assert.ok(Math.abs(sections.base_web_section.area-sections.base_relief_section.area-2*pocketArea)<1e-6);
  const sheet=await json('drawing-base_relief_machining.json');
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(manifest.drawing_sheets.base_relief_machining.status,'published');
  assert.ok((await page.goto(`${base}/drawing-base_relief_machining.svg`)).ok());
  const visual=await page.evaluate(()=>{
   const root=document.documentElement,box=e=>{const b=e.getBoundingClientRect();return {left:b.left,right:b.right,top:b.top,bottom:b.bottom};};
   return {paper:box(root),views:[...root.querySelectorAll('svg[id^="drawing-view-"]')].map(v=>({id:v.id,box:box(v),paths:[...v.querySelectorAll('path')].map(p=>p.getAttribute('d'))})),
    rows:[...root.querySelectorAll('g[data-dimension]')].map(r=>({id:r.dataset.dimension,text:r.textContent,box:box(r),labels:[...r.querySelectorAll('text')].map(t=>box(t))})),text:root.textContent};
  });
  assert.deepEqual(new Set(visual.views.map(v=>v.id)),new Set(['drawing-view-base_relief_section','drawing-view-base_web_section']));
  for(const view of visual.views){assert.ok(view.paths.some(p=>p?.includes('A ')),'actual native capsule contours visible');
   assert.ok(view.box.left>=visual.paper.left-1&&view.box.right<=visual.paper.right+1&&view.box.top>=visual.paper.top-1&&view.box.bottom<=visual.paper.bottom+1);}
  for(const [id,value] of Object.entries(expected)){
   const row=visual.rows.find(r=>r.id===id);assert.ok(row,`${id} visible row`);
   assert.ok(row.text.includes(`≈ ${value.toFixed(3)} mm`),`${id} actual measured depth`);
   const report=sheet.dimension_tables.flatMap(t=>t.rows).find(r=>r.dimension===id);
   assert.deepEqual(report.native_dimension,datum.dimensions[id]);
   for(const label of row.labels)assert.ok(label.left>=row.box.left-1&&label.right<=row.box.right+1&&label.top>=row.box.top-1&&label.bottom<=row.box.bottom+1);
  }
  assert.ok(visual.text.includes(manifest.recipe_context_hash));
  const cdp=await page.context().newCDPSession(page);
  fs.writeFileSync('/tmp/cad-pocketed-drive-drawing.png',Buffer.from((await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true})).data,'base64'));
  await cdp.detach();
  assert.ok((await page.goto(`${base}/drive_module_release.gpu.html`,{waitUntil:'domcontentloaded'})).ok());
  await page.locator('#recipe-toggle').click();
  await page.locator('#occurrence-index').fill(String(baseIndex));await page.locator('#inspect-index').click();
  const spanText=span=>Buffer.from(source).subarray(span.start_byte,span.end_byte).toString('utf8');
  const checkSpan=async span=>{
   assert.equal(span.file,'drive_module_rotation.star');
   await page.waitForFunction(s=>{const r=document.querySelector('#recipe-source');return r?.dataset.sourceFile===s.file&&r?.dataset.startByte===String(s.start_byte);},span);
   assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(span));
  };
  await checkSpan(provenance.parts.base.declaration);
  assert.ok((await page.locator('#selection').textContent()).includes('base'));
  await page.locator('#recipe-feature-source').click();
  await checkSpan(provenance.features.base_deep_relief.declaration);
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);
  await page.screenshot({path:'/tmp/cad-pocketed-drive-assembly.png'});
  assert.ok((await page.goto(`${base}/base.preview.html`,{waitUntil:'domcontentloaded'})).ok());
  await page.locator('canvas').first().waitFor({state:'visible'});
  await page.screenshot({path:'/tmp/cad-pocketed-drive-base.png'});
  const manifestStart=captured['manifest.json'];await bytes('manifest.json');
  assert.equal(captured['manifest.json'],manifestStart,'pinned manifest generation');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'pinned browser acceptance script');
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,read_only:true,actual_assembly_base_source:true,native_depth_dimensions:3,
   through_slots:4,blind_pockets:2,moving_occurrences:9,named_clearance_pairs:13,
   recipe_context_hash:manifest.recipe_context_hash,script_sha256:scriptSha,artifacts_sha256:captured,
   screenshots:['/tmp/cad-pocketed-drive-drawing.png','/tmp/cad-pocketed-drive-assembly.png','/tmp/cad-pocketed-drive-base.png']},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
