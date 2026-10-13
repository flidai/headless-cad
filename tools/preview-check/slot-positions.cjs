// Browser acceptance for the source-owned native slot-center machining sheet.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_slot_positions';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const sourceFor=(bundle,label)=>label===bundle.source_name?bundle.source:bundle.modules[label];
const spanText=(bundle,span)=>Buffer.from(sourceFor(bundle,span.file),'utf8')
 .subarray(span.start_byte,span.end_byte).toString('utf8');
const expected={slot_positions:{front_x:'X1',rear_x:'X2',lower_y:'Y1',upper_y:'Y2'},
 slot_pitches:{pitch_x:'PX',pitch_y:'PY'}};

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[];
 try{
  const page=await browser.newPage({viewport:{width:2400,height:1800},deviceScaleFactor:1});
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(!['GET','HEAD'].includes(r.method()))writes.push(`${r.method()} ${r.url()}`);});
  const get=async name=>{const response=await page.request.get(`${base}/${name}`);assert.equal(response.status(),200,name);return response;};
  const manifestBytes=await(await get('manifest.json')).body();
  const manifest=JSON.parse(manifestBytes.toString('utf8'));
  const bundle=await(await get('recipe.bundle.json')).json();
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const sheet=await(await get('drawing-base_slot_positions.json')).json();
  assert.equal(manifest.recipe_source.name,'drive_module_slot_positions.star');
  assert.equal(bundle.source_name,manifest.recipe_source.name);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.source_bundle_hash,manifest.recipe_bundle.source_bundle_hash);
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(manifest.source_provenance.artifact,'source-provenance.json');
  for(const [label,item] of Object.entries(provenance.source_inventory)){
   const source=sourceFor(bundle,label);assert.equal(typeof source,'string',label);
   const bytes=Buffer.from(source,'utf8');
   assert.equal(bytes.length,item.bytes,`${label} captured byte count`);
   assert.equal(sha(bytes),item.sha256,`${label} captured source digest`);
  }
  const sections={};
  for(const view of Object.keys(expected)){
   sections[view]=await(await get(`section-${view}.json`)).json();
   assert.equal(sections[view].snapshot.recipe_source_hash,manifest.recipe_source_hash);
   const dimensions=sections[view].section_view.dimensions;
   assert.deepEqual(dimensions.map(d=>d.declaration.id).sort(),Object.keys(expected[view]).sort());
   for(const dimension of dimensions){
    assert.equal(dimension.status,'passed');
    assert.ok(Number.isFinite(dimension.value_mm)&&dimension.value_mm>0);
    for(const end of ['from_witness','to_witness']){
     const witness=dimension[end];
     assert.deepEqual(witness.instance_path,['machining_leaf']);
     assert.equal(witness.part_id,'machining_base');
     if(['front_minus','front_plus','rear_minus','rear_plus'].includes(witness.datum)){
      assert.ok(witness.slot_datum,`${dimension.declaration.id} ${end} retains native slot witness`);
      assert.equal(witness.slot_datum.definition.role,`slot_${witness.datum}_walls`);
     }
    }
   }
  }
  const drawing=await page.goto(`${base}/drawing-base_slot_positions.svg`,{waitUntil:'load'});
  assert.ok(drawing?.ok(),'native machining sheet loads');
  const visual=await page.evaluate(()=>{
   const root=document.documentElement;
   const clip=(line,box)=>{
    const r=line.halfStroke,x0=line.from.x,y0=line.from.y;
    const dx=line.to.x-x0,dy=line.to.y-y0;
    let low=0,high=1;
    for(const [p,q] of [[-dx,x0-box.left+r],[dx,box.right-x0+r],
      [-dy,y0-box.top+r],[dy,box.bottom-y0+r]]){
     if(Math.abs(p)<1e-12){if(q<0)return false;continue;}
     const bound=q/p;
     if(p<0)low=Math.max(low,bound);else high=Math.min(high,bound);
     if(low>high)return false;
    }
    return true;
   };
   const views={};
   for(const id of ['slot_positions','slot_pitches']){
    const viewport=document.querySelector(`#drawing-view-${id}`);
    if(!viewport)throw Error(`missing ${id} viewport`);
    const rect=viewport.getBoundingClientRect(),viewBox=viewport.viewBox.baseVal;
    const groups=[...viewport.querySelectorAll('g.section-dimension')];
    const labels=groups.map(group=>{
     const text=group.querySelector('text');
     if(!text)throw Error(`missing text in ${id} dimension`);
     const box=text.getBoundingClientRect(),style=getComputedStyle(text);
     return {id:group.getAttribute('data-id'),value:Number(group.getAttribute('data-value-mm')),
      text:text.textContent,fontModel:Number(text.getAttribute('font-size')),
      rect:{left:box.left,top:box.top,right:box.right,bottom:box.bottom,width:box.width,height:box.height},
      visible:style.display!=='none'&&style.visibility==='visible'&&Number(style.opacity)>0,
      lines:group.querySelectorAll('line').length,arrows:group.querySelectorAll('polygon').length};
    });
    const lines=groups.flatMap(group=>[...group.querySelectorAll('line')].map((line,index)=>{
     const matrix=line.getScreenCTM();if(!matrix)throw Error('dimension line lacks screen transform');
     const from=new DOMPoint(line.x1.baseVal.value,line.y1.baseVal.value).matrixTransform(matrix);
     const to=new DOMPoint(line.x2.baseVal.value,line.y2.baseVal.value).matrixTransform(matrix);
     const stroke=Number(line.getAttribute('stroke-width')||group.getAttribute('stroke-width'))*
      Math.max(Math.hypot(matrix.a,matrix.b),Math.hypot(matrix.c,matrix.d));
     return {id:group.getAttribute('data-id'),index,from:{x:from.x,y:from.y},to:{x:to.x,y:to.y},halfStroke:stroke/2};
    }));
    const collisions=labels.flatMap(label=>lines.filter(line=>clip(line,label.rect))
     .map(line=>({label:label.id,line:line.id,index:line.index})));
    views[id]={viewport:{left:rect.left,top:rect.top,right:rect.right,bottom:rect.bottom,
      width:rect.width,height:rect.height},
     paperScale:Number(viewport.getAttribute('width'))/viewBox.width,
     declaredScale:Number(viewport.getAttribute('data-scale')),
     declaredFont:Number(viewport.getAttribute('data-paper-annotation-font-mm')),
     labels,collisions};
   }
   return {paper:{width:root.width.baseVal.valueInSpecifiedUnits,
    height:root.height.baseVal.valueInSpecifiedUnits},views};
  });
  assert.deepEqual(visual.paper,{width:594,height:420});
  const allLabels=[];
  for(const [view,expectedTags] of Object.entries(expected)){
   const {viewport,paperScale,declaredScale,declaredFont,labels,collisions}=visual.views[view];
   assert.deepEqual(labels.map(label=>label.id).sort(),Object.keys(expectedTags).sort());
   assert.ok(Math.abs(paperScale-declaredScale)<1e-6,`${view} paper scale`);
   assert.equal(declaredFont,2.8,`${view} declared paper font`);
   assert.deepEqual(collisions,[],`${view} label clears every dimension/extension line`);
   for(const label of labels){
    const report=sections[view].section_view.dimensions.find(d=>d.declaration.id===label.id);
    assert.ok(report,label.id);
    assert.ok(Math.abs(label.value-report.value_mm)<1e-8,`${label.id} DOM report value`);
    assert.ok(label.visible&&label.rect.width>0&&label.rect.height>0,`${label.id} visible`);
    assert.ok(label.text.includes(`${expectedTags[label.id]}:`),`${label.id} authored paper tag`);
    assert.ok(label.text.includes('mm'),`${label.id} unit`);
    assert.equal(label.lines,3,`${label.id} extension and dimension lines`);
    assert.equal(label.arrows,2,`${label.id} arrowheads`);
    assert.ok(Math.abs(label.fontModel*paperScale-2.8)<2e-5,`${label.id} physical font`);
    const b=label.rect,v=viewport,epsilon=.75;
    assert.ok(b.left>=v.left-epsilon&&b.right<=v.right+epsilon&&b.top>=v.top-epsilon&&b.bottom<=v.bottom+epsilon,`${label.id} inside its paper viewport`);
    allLabels.push({...label,view});
   }
  }
  assert.equal(allLabels.length,6);
  for(let i=0;i<allLabels.length;i++)for(let j=i+1;j<allLabels.length;j++){
   const a=allLabels[i].rect,b=allLabels[j].rect;
   assert.equal(a.left<b.right&&b.left<a.right&&a.top<b.bottom&&b.top<a.bottom,
    false,`${allLabels[i].id}/${allLabels[j].id} actual label bboxes do not overlap`);
  }
  const cdp=await page.context().newCDPSession(page);
  const capture=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});
  fs.writeFileSync('/tmp/cad-slot-positions.png',Buffer.from(capture.data,'base64'));
  await cdp.detach();

  const preview=await page.goto(`${base}/base_machining_layout.gpu.html`,{waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'one-leaf machining assembly GPU inspection loads');
  await page.locator('#recipe-toggle').click();
  await page.locator('#occurrence-index').fill('0');
  await page.locator('#inspect-index').click();
  const partSpan=provenance.parts.machining_base.declaration;
  assert.equal(partSpan.file,bundle.source_name,'part declaration belongs to root recipe');
  await page.waitForFunction(span=>{
   const source=document.querySelector('#recipe-source');
   return source?.dataset.sourceFile===span.file&&source?.dataset.startByte===String(span.start_byte);
  },partSpan);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,partSpan));
  assert.equal(await page.locator('#recipe-module').inputValue(),'','root source selected for picked part');
  assert.ok((await page.locator('#selection').textContent()).includes('machining_base'));
  assert.ok((await page.locator('#part-datums').textContent()).includes('front_minus'));
  const bodyFeature=provenance.part_body_features.machining_base;
  const bodySpan=provenance.features[bodyFeature].declaration;
  assert.equal(bodySpan.file,'drive_module_rotation.star','body feature is captured imported source');
  await page.locator('#recipe-feature-source').click();
  await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),bodySpan);
  assert.equal(await page.locator('#recipe-module').inputValue(),bodySpan.file);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,bodySpan));
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null,'captured source remains read-only');
  const manifestEndBytes=await(await get('manifest.json')).body();
  assert.equal(sha(manifestEndBytes),sha(manifestBytes),'browser stayed on one manifest revision');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'browser script stayed pinned');
  assert.deepEqual(errors,[],'zero browser script errors');
  assert.deepEqual(writes,[],'no write requests');
  console.log(JSON.stringify({valid:true,dimensions:6,viewports:2,paper_font_mm:2.8,
   no_label_overlaps:true,no_line_label_crossings:true,read_only:true,
   manifest_sha256:sha(manifestBytes),manifest_end_sha256:sha(manifestEndBytes),
   script_sha256:scriptSha,recipe_context_hash:manifest.recipe_context_hash,
   captured_source_sha256:sha(Buffer.from(bundle.source,'utf8')),
   provenance_sha256:sha(provenanceBytes),screenshot:'/tmp/cad-slot-positions.png'},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
