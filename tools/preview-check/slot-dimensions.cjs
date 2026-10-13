// Browser acceptance for the captured native slot machining sheet and source.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_slot_drawing';
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const sourceFor=(bundle,label)=>label===bundle.source_name?bundle.source:bundle.modules[label];

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[];
 try{
  const page=await browser.newPage({viewport:{width:1800,height:1300},deviceScaleFactor:1});
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(!['GET','HEAD'].includes(r.method()))writes.push(`${r.method()} ${r.url()}`);});
  const get=async name=>{const response=await page.request.get(`${base}/${name}`);assert.equal(response.status(),200,name);return response;};
  const manifestBytes=await(await get('manifest.json')).body();
  const manifest=JSON.parse(manifestBytes.toString('utf8'));
  const bundle=await(await get('recipe.bundle.json')).json();
  const provenanceBytes=await(await get('source-provenance.json')).body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const section=await(await get('section-base_slot_plan.json')).json();
  const sheet=await(await get('drawing-base_slot_machining.json')).json();
  assert.equal(manifest.recipe_source.name,'drive_module_slot_drawing.star');
  assert.equal(bundle.source_name,manifest.recipe_source.name);
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.source_bundle_hash,manifest.recipe_bundle.source_bundle_hash);
  assert.equal(section.snapshot.recipe_source_hash,manifest.recipe_source_hash);
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(manifest.source_provenance.artifact,'source-provenance.json');
  for(const [label,item] of Object.entries(provenance.source_inventory)){
   const source=sourceFor(bundle,label);assert.equal(typeof source,'string',label);
   const bytes=Buffer.from(source,'utf8');
   assert.equal(bytes.length,item.bytes,`${label} byte count`);
   assert.equal(sha(bytes),item.sha256,`${label} captured source digest`);
  }
  const dimensions=section.section_view.slot_dimensions;
  assert.equal(dimensions.length,12,'four slots times three measured dimensions');
  const byId=new Map(dimensions.map(d=>[d.declaration.id,d]));
  assert.equal(byId.size,12,'dimension IDs unique');
  const expected=[];
  for(const name of ['front_minus','front_plus','rear_minus','rear_plus']){
   for(const [suffix,kind] of [['width','width'],['overall','overall_length'],['centerline','centerline_length']]){
    const id=`${name}_${suffix}`;expected.push(id);
    const d=byId.get(id);assert.ok(d,id);assert.equal(d.declaration.kind,kind);
    assert.equal(d.declaration.role,`slot_${name}_walls`);
    assert.equal(d.status,'passed');
    assert.equal(d.measurement.segments.length,4);
   }
  }
  assert.deepEqual([...byId.keys()].sort(),expected.sort());
  const drawingResponse=await page.goto(`${base}/drawing-base_slot_machining.svg`,{waitUntil:'load'});
  assert.ok(drawingResponse?.ok(),'paper drawing loads');
  const visual=await page.evaluate(()=>{
   const root=document.documentElement;
   const viewport=document.querySelector('#drawing-view-base_slot_plan');
   if(!viewport)throw Error('native slot drawing viewport missing');
   const rect=viewport.getBoundingClientRect();
   const viewBox=viewport.viewBox.baseVal;
   const paperScale=Number(viewport.getAttribute('width'))/viewBox.width;
   const groups=[...viewport.querySelectorAll('g.slot-dimension')];
   const labels=groups.map(group=>{
    const text=group.querySelector('text');
    if(!text)throw Error('slot dimension has no visible text');
    const box=text.getBoundingClientRect(),style=getComputedStyle(text);
    return {id:group.getAttribute('data-dimension-id'),kind:group.getAttribute('data-kind'),
      text:text.textContent,rect:{left:box.left,top:box.top,right:box.right,bottom:box.bottom,width:box.width,height:box.height},
      fontModel:Number(text.getAttribute('font-size')),
      visible:style.display!=='none'&&style.visibility==='visible'&&Number(style.opacity)>0,
      marks:group.querySelectorAll('path.slot-center-mark').length,
      lines:group.querySelectorAll('line').length,arrows:group.querySelectorAll('polygon').length};
   });
   const lines=groups.flatMap(group=>[...group.querySelectorAll('line')].map((line,index)=>{
    const transform=line.getScreenCTM();
    if(!transform)throw Error('slot dimension line has no screen transform');
    const from=new DOMPoint(line.x1.baseVal.value,line.y1.baseVal.value).matrixTransform(transform);
    const to=new DOMPoint(line.x2.baseVal.value,line.y2.baseVal.value).matrixTransform(transform);
    const stroke=Number(line.getAttribute('stroke-width')||group.getAttribute('stroke-width'))*
      Math.max(Math.hypot(transform.a,transform.b),Math.hypot(transform.c,transform.d));
    return {id:group.getAttribute('data-dimension-id'),index,
      from:{x:from.x,y:from.y},to:{x:to.x,y:to.y},halfStroke:stroke/2};
   }));
   // Liang-Barsky clipping in the same screen coordinates as text.getBoundingClientRect().
   // Inflate by half the rendered line stroke so touching ink also fails.
   const crosses=(line,box)=>{
    const r=line.halfStroke,x0=line.from.x,y0=line.from.y;
    const dx=line.to.x-x0,dy=line.to.y-y0;
    let low=0,high=1;
    for(const [p,q] of [[-dx,x0-box.left+r],
      [dx,box.right-x0+r],[-dy,y0-box.top+r],[dy,box.bottom-y0+r]]){
     if(Math.abs(p)<1e-12){if(q<0)return false;continue;}
     const bound=q/p;
     if(p<0)low=Math.max(low,bound);else high=Math.min(high,bound);
     if(low>high)return false;
    }
    return true;
   };
   const lineCollisions=labels.flatMap(label=>lines.filter(line=>crosses(line,label.rect))
     .map(line=>({label:label.id,line:line.id,index:line.index})));
   return {rootWidth:root.width.baseVal.valueInSpecifiedUnits,rootHeight:root.height.baseVal.valueInSpecifiedUnits,
     viewport:{left:rect.left,top:rect.top,right:rect.right,bottom:rect.bottom,width:rect.width,height:rect.height},
     paperScale,declaredScale:Number(viewport.getAttribute('data-scale')),labels,lineCollisions};
  });
  assert.equal(visual.rootWidth,420);assert.equal(visual.rootHeight,297);
  assert.equal(visual.labels.length,12,'twelve actual SVG annotation groups');
  assert.ok(Math.abs(visual.paperScale-visual.declaredScale)<1e-6,'native drawing respects declared paper scale');
  assert.deepEqual(visual.labels.map(label=>label.id).sort(),expected);
  const tags={front_minus:'FM',front_plus:'FP',rear_minus:'RM',rear_plus:'RP'};
  for(const label of visual.labels){
   const d=byId.get(label.id),name=label.id.replace(/_(width|overall|centerline)$/,'');
   const suffix={width:'W',overall:'O',centerline:'C'}[label.id.split('_').at(-1)];
   assert.equal(label.kind,d.declaration.kind,label.id);
   assert.ok(label.visible&&label.rect.width>0&&label.rect.height>0,`${label.id} visible`);
   assert.ok(label.text.includes(`${tags[name]}${suffix}:`),`${label.id} native tag`);
   assert.ok(label.text.includes('mm'),`${label.id} unit`);
   assert.equal(label.marks,3,`${label.id} center and endpoint marks`);
   assert.equal(label.lines,3,`${label.id} extension and dimension lines`);
   assert.equal(label.arrows,2,`${label.id} arrowheads`);
   assert.ok(Math.abs(label.fontModel*visual.paperScale-2.8)<2e-5,`${label.id} physical font height`);
   const b=label.rect,v=visual.viewport,epsilon=.75;
   assert.ok(b.left>=v.left-epsilon&&b.right<=v.right+epsilon&&b.top>=v.top-epsilon&&b.bottom<=v.bottom+epsilon,`${label.id} inside viewport`);
  }
  for(let i=0;i<visual.labels.length;i++)for(let j=i+1;j<visual.labels.length;j++){
   const a=visual.labels[i].rect,b=visual.labels[j].rect;
   const overlap=a.left<b.right&&b.left<a.right&&a.top<b.bottom&&b.top<a.bottom;
   assert.equal(overlap,false,`${visual.labels[i].id}/${visual.labels[j].id} visible labels do not overlap`);
  }
  assert.deepEqual(visual.lineCollisions,[],'all slot labels clear every dimension and extension line');
  const cdp=await page.context().newCDPSession(page);
  const capture=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  fs.writeFileSync('/tmp/cad-slot-dimensions-final.png',Buffer.from(capture.data,'base64'));
  await cdp.detach();

  const preview=await page.goto(`${base}/base.preview.html`,{waitUntil:'domcontentloaded'});
  assert.ok(preview?.ok(),'slotted base preview loads');
  await page.locator('#recipe-toggle').click();
  await page.waitForFunction(text=>document.querySelector('#recipe-source')?.textContent===text,bundle.modules['drive_module_rotation.star']);
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.modules['drive_module_rotation.star'],'preview follows defining imported body source');
  assert.equal(await page.locator('#recipe-module option').count(),2,'root plus one locked module source');
  await page.locator('#recipe-module').selectOption('');
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.source,'root drawing recipe is inspectable');
  await page.locator('#recipe-module').selectOption('drive_module_rotation.star');
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.modules['drive_module_rotation.star']);
  assert.ok((await page.locator('#recipe-source').textContent()).includes('base_slotted'));
  assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null,'captured source is read-only');
  const canvas=page.locator('#view');
  const box=await canvas.boundingBox();assert.ok(box,'base canvas visible');
  await canvas.click({position:{x:box.width*.5,y:box.height*.5}});
  assert.ok((await page.locator('#selection').textContent()).includes('Face'),'body face inspection works');
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.modules['drive_module_rotation.star'],'body selection preserves selected captured module');
  const manifestEndBytes=await(await get('manifest.json')).body();
  assert.equal(sha(manifestEndBytes),sha(manifestBytes),'browser run pinned to one manifest revision');
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha,'browser script unchanged');
  assert.deepEqual(errors,[],'no browser script errors');
  assert.deepEqual(writes,[],'inspection made no write requests');
  console.log(JSON.stringify({valid:true,script_sha256:scriptSha,manifest_end_sha256:sha(manifestEndBytes),dimensions:visual.labels.length,paper_scale:visual.paperScale,
   paper_font_mm:2.8,no_label_overlaps:true,no_line_label_crossings:true,captured_source_sha256:sha(Buffer.from(bundle.source,'utf8')),
   provenance_sha256:sha(provenanceBytes),manifest_sha256:sha(manifestBytes),recipe_context_hash:manifest.recipe_context_hash,
   read_only:true,screenshot:'/tmp/cad-slot-dimensions-final.png'},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
