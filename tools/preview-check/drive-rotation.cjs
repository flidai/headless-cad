// Source-bound rigid display motion, with no authoring or model execution.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_rotation';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/rigid-rotation-groups.html`)).ok());
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.ok((await page.request.get(link)).ok(),link);
  const evidence=await(await page.request.get(`${base}/rigid-rotation-groups.json`)).json();
  const group=evidence.groups.rotor;assert.equal(group.members.length,9);assert.equal(group.coverage,'named_pairs_only');
  assert.equal(group.clearance_check_ids.length,13);
  for(const name of ['left_cover_shaft_gap','right_cover_shaft_gap'])
   assert.ok(group.clearance_check_ids.includes(name),`native cover-shaft proof ${name}`);
  const assembly=await(await page.request.get(`${base}/drive_module_release.assembly.json`)).json();
  assert.equal(assembly.instances.length,30);assert.equal(Object.keys(assembly.definitions).length,14);
  const added=[
   ['left_support','left_cover'],['right_support','right_cover'],
   ...['left','right'].flatMap(side=>['minus','plus'].map(tag=>[`${side}_support`,`${side}_cover_screw_${tag}`])),
  ];
  for(const path of added)assert.ok(assembly.instances.some(i=>i.id===JSON.stringify(path)),`placed ${path.join('/')}`);
  const source=await(await page.request.get(`${base}/recipe.star`)).text();
  assert.ok(source.includes('left_cover_shaft_gap')&&source.includes('right_cover_shaft_gap'));
  const section=await(await page.request.get(`${base}/section-mounting_plan.json`)).json();
  const baseSection=section.occurrences.find(o=>o.instance_path.length===1&&o.instance_path[0]==='base');
  assert.ok(baseSection,'base occurrence has a native mounting section');
  const radius=6,features=['base_front_left','base_front_right','base_back_right','base_body'];
  assert.match(source,/base_corner_radius\s*=\s*param\(name\s*=\s*"base_corner_radius",\s*default\s*=\s*6\)/);
  assert.ok(source.includes('coverage = "single_arc"')&&source.includes('role = "fillet_wall"'));
  for(const feature of features)assert.ok(source.includes(`fillet(id = "${feature}"`),`${feature} is source-authored`);
  const loops=baseSection.section.loops;
  const outer=loops.find(loop=>loop.segments.filter(s=>s.curve.kind==='arc'&&Math.abs(s.curve.sweep_angle)<2*Math.PI-1e-8).length===4);
  assert.ok(outer,'base outer loop has four native rounded corners');
  assert.equal(loops.filter(loop=>loop.segments.length===1&&loop.segments[0].curve.kind==='arc'&&Math.abs(Math.abs(loop.segments[0].curve.sweep_angle)-2*Math.PI)<1e-8).length,4,'four mounting holes survive');
  const rounded=outer.segments.filter(s=>s.curve.kind==='arc');
  assert.equal(rounded.length,4);
  assert.ok(rounded.every(s=>Math.abs(s.curve.radius-radius)<1e-9&&Math.abs(Math.abs(s.curve.sweep_angle)-Math.PI/2)<1e-9));
  const reports=section.section_view.circle_dimensions;
  assert.equal(reports.length,4);
  assert.equal(section.section_views.mounting_plan.circle_dimensions.length,4);
  for(const feature of features){
   const id=`${feature}_radius`,report=reports.find(r=>r.declaration.id===id),summary=section.section_views.mounting_plan.circle_dimensions.find(r=>r.id===id);
   assert.ok(report&&summary,`${id} has native and published reports`);
   assert.equal(report.status,'passed');assert.equal(summary.status,'passed');
   assert.equal(report.declaration.anchor_feature,feature);assert.equal(report.declaration.role,'fillet_wall');
   assert.equal(report.declaration.coverage,'single_arc');assert.equal(report.declaration.kind,'radius');
   assert.deepEqual(report.instance_path,['base']);assert.equal(report.target_content_hash,baseSection.content_hash);
   assert.equal(report.radius,radius);assert.equal(report.value_mm,radius);assert.equal(summary.radius,radius);
   assert.equal(report.selection.status,'resolved');assert.equal(report.selection.witnesses.length,1);
   const arc=report.arc,segment=loops[arc.loop_index]?.segments[arc.segment_index];
   assert.ok(segment&&segment.curve.kind==='arc',`${id} targets a native arc`);
   assert.equal(segment.face,arc.target_face);assert.equal(report.selection.witnesses[0].target_face,arc.target_face);
   assert.deepEqual(segment.curve.center,report.center);assert.equal(segment.curve.radius,report.radius);
   assert.equal(segment.curve.start_angle,arc.start_angle);assert.equal(segment.curve.sweep_angle,arc.sweep_angle);
   const mid=arc.start_angle+arc.sweep_angle/2;
   assert.ok(Math.hypot(arc.tip[0]-report.center[0]-radius*Math.cos(mid),arc.tip[1]-report.center[1]-radius*Math.sin(mid))<1e-8);
  }
  const svg=await(await page.request.get(`${base}/section-mounting_plan.svg`)).text();
  const annotation=await page.evaluate(xml=>{
   const doc=new DOMParser().parseFromString(xml,'image/svg+xml');
   if(doc.querySelector('parsererror'))throw Error('mounting SVG parse failed');
   return {metadata:JSON.parse(doc.querySelector('metadata').textContent),groups:[...doc.querySelectorAll('g.section-circle-dimension')].map(g=>({id:g.getAttribute('data-id'),kind:g.getAttribute('data-kind'),value:Number(g.getAttribute('data-value-mm')),text:g.querySelector('text')?.textContent,lines:[...g.querySelectorAll('line')].map(line=>[...['x1','y1','x2','y2'].map(a=>Number(line.getAttribute(a)))])}))};
  },svg);
  assert.equal(annotation.metadata.snapshot.recipe_source_hash,section.snapshot.recipe_source_hash);
  assert.equal(annotation.groups.length,4);
  for(const feature of features){
   const id=`${feature}_radius`,r=reports.find(r=>r.declaration.id===id),g=annotation.groups.find(g=>g.id===id);
   assert.ok(g&&g.lines.length>=2,`${id} has an arrow and elbow leader`);
   assert.equal(g.kind,'radius');assert.equal(g.value,r.radius);assert.ok(g.text.includes(`${id}: ${radius} mm radius`));
   const [cx,cy,tx,ty]=g.lines[0];
   assert.ok(Math.hypot(cx-r.center[0],cy+r.center[1])<1e-8);
   assert.ok(Math.hypot(tx-r.arc.tip[0],ty+r.arc.tip[1])<1e-8,`${id} arrow touches native arc midpoint`);
  }
  assert.ok((await page.goto(`${base}/rotation-rotor.html`)).ok());
  assert.equal(await page.locator('#rotation-inspection').getAttribute('data-error'),null);
  const before=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{
   const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];
  })));
  const members=new Set(group.members.map(member=>member.instance_id)),axis=group.rotation_world.axis,pivot=group.rotation_world.pivot;
  function turn(vector,angle){const [x,y,z]=axis,[a,b,c]=vector,cross=[y*c-z*b,z*a-x*c,x*b-y*a],dot=x*a+y*b+z*c;return vector.map((v,i)=>v*Math.cos(angle)+cross[i]*Math.sin(angle)+axis[i]*dot*(1-Math.cos(angle)));}
  for(const phase of [.125,.25,.5,1]){
   await page.locator('#rotation-phase').evaluate((slider,value)=>{slider.value=String(value);slider.dispatchEvent(new Event('input',{bubbles:true}));},phase);
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),String(phase));
   const after=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];})));
   for(const [id,start] of Object.entries(before)){
    if(!members.has(id)){assert.deepEqual(after[id],start,`stationary ${id}`);continue;}
    const expected=turn(start.point.map((v,i)=>v-pivot[i]),phase*group.rotation_world.angle_radians).map((v,i)=>v+pivot[i]);
    const normal=turn(start.normal,phase*group.rotation_world.angle_radians);
    for(let i=0;i<3;i++){assert.ok(Math.abs(after[id].point[i]-expected[i])<1e-10,`point ${id}`);assert.ok(Math.abs(after[id].normal[i]-normal[i])<1e-10,`normal ${id}`);}
   }
  }
  await page.locator('#rotation-reset').click();
  const reset=await page.evaluate(()=>Object.fromEntries(scene.instances.map(instance=>{const face=scene.faces.find(face=>face.instance_id===instance.id);return [instance.id,{point:face.points[0],normal:face.normal}];})));
  assert.deepEqual(reset,before);
  await page.locator('#recipe-toggle').click();
  const options=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})));
  for(const name of ['shaft','parallel_key','hub_left_ring','shaft_ring',
   'left_cover','right_cover','left_cover_screw_minus','right_cover_screw_plus']){
   const option=options.find(o=>o.text.includes(name));assert.ok(option,name);await page.locator('#occurrence-select').selectOption(option.value);
   assert.equal(await page.locator('#recipe-source').textContent(),source);assert.ok((await page.locator('#recipe-summary').textContent()).includes(name));
  }
  assert.ok(source.includes('hole_pattern(id = feature_id'));
  assert.ok(source.includes('id = "end_cover_body"'));
  assert.ok(source.includes('id = "cover_screw_body"'));
  const picked=[];
  for(const [side,paths] of [['left',added.filter(path=>path[0]==='left_support')],
                            ['right',added.filter(path=>path[0]==='right_support')]]){
   for(const path of paths){
    const target=JSON.stringify(path);
    const point=await page.evaluate(({side,target})=>{
     yaw=side==='left'?Math.PI:0;pitch=0;zoom=1;draw();
     const owns=(x,y)=>x>=0&&y>=0&&x<bufferWidth&&y<bufferHeight&&
      scene.faces[faceBuffer[y*bufferWidth+x]]?.instance_id===target;
     for(let y=2;y<bufferHeight-2;y++)for(let x=2;x<bufferWidth-2;x++){
      if(owns(x,y)&&owns(x-1,y)&&owns(x+1,y)&&owns(x,y-1)&&owns(x,y+1))
       return[(x+.5)/renderRatio,(y+.5)/renderRatio];
     }
     return null;
    },{side,target});
    assert.ok(point,`visible pickable surface for ${path.join('/')}`);
    await page.locator('#view').click({position:{x:point[0],y:point[1]}});
    const selection=await page.locator('#selection').textContent();
    assert.ok(selection.includes(path.map(JSON.stringify).join(' / ')),
     `picked ${path.join('/')}: ${selection}`);
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    assert.ok((await page.locator('#recipe-summary').textContent()).includes(path.at(-1)));
    picked.push(path.join('/'));
   }
  }
  await page.locator('#rotation-phase').evaluate(slider=>{slider.value='.25';slider.dispatchEvent(new Event('input',{bubbles:true}));});
  await page.waitForTimeout(1200);
  assert.equal(await page.locator('.badge').textContent(),'READ-ONLY MOTION');
  assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),'0.25');
  assert.equal(await page.locator('#recipe-source').textContent(),source);
  const capture=async path=>{const cdp=await page.context().newCDPSession(page),shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(path,Buffer.from(shot.data,'base64'));await cdp.detach();};
  await capture('/tmp/cad-drive-rotation.png');
  assert.ok((await page.goto(`${base}/tolerance-stacks.html`)).ok());await capture('/tmp/cad-drive-rotation-stacks.png');
  await page.setContent(`<html><body style="margin:0;background:#e7edf5;display:flex;justify-content:center;align-items:flex-start"><img id="drawing" src="${base}/section-mounting_plan.svg" style="display:block;max-width:100%;height:auto;background:white"></body></html>`);
  await page.locator('#drawing').evaluate(img=>img.decode());
  await capture('/tmp/cad-rounded-drive-drawing.png');
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,moving_occurrences:9,stationary_occurrences:21,
   added_picked_occurrences:picked,rounded_corners:4,radius_annotations:4,
   phases:[.125,.25,.5,1],captured_source:true,read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
