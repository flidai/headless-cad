// Read-only acceptance of actual pocketed-base mounting and continuous adjustment.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs'),crypto=require('node:crypto');
const root=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1600,height:1100}}),errors=[],writes=[],results=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(!['GET','HEAD'].includes(r.method()))writes.push(`${r.method()} ${r.url()}`);});
  for(const mode of ['tightened','adjustment']){
   const base=`${root}/drive_module_pocketed_${mode}`,captured={};
   const bytes=async name=>{const r=await page.request.get(`${base}/${name}`);assert.equal(r.status(),200,name);
    const b=await r.body();captured[name]=sha(b);return b;};
   const json=async name=>JSON.parse((await bytes(name)).toString('utf8'));
   const manifest=await json('manifest.json'),bundle=await json('recipe.bundle.json'),provenance=await json('source-provenance.json');
   const manifestSha=captured['manifest.json'];
   assert.equal(bundle.source_name,`drive_module_${mode}.star`);
   assert.equal(manifest.recipe_source.parameter_overrides.base_pocket_depth,1);
   assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
   assert.equal(provenance.source_bundle_hash,manifest.source_bundle_hash);
   const textFor=file=>file===bundle.source_name?bundle.source:bundle.modules[file];
   for(const [file,item] of Object.entries(provenance.source_inventory)){
    const content=Buffer.from(textFor(file));assert.equal(content.length,item.bytes);assert.equal(sha(content),item.sha256);
   }
   assert.ok(bundle.modules['drive_module_rotation.star'].includes('base_deep_relief'));
   const output=`drive_module_${mode}`;
   assert.equal(manifest.outputs[output].instances,30);
   const checks=await json('clearance-checks.json');
   assert.equal(checks.recipe_context_hash,manifest.recipe_context_hash);
   let group=null;
   if(mode==='tightened'){
    const seats=await json('nominal-seating-checks.json');
    assert.deepEqual(seats.checks,manifest.nominal_seating_checks);
    const extraSeats=Object.entries(seats.checks).filter(([id])=>id.startsWith('tight_'));
    assert.equal(extraSeats.length,8);
    for(const [id,s] of extraSeats){assert.equal(s.status,'passed',id);assert.ok(s.disk_radius_lower_mm>=.4,id);}
    const gaps=Object.entries(checks.checks).filter(([id])=>id.startsWith('tight_slot_'));
    assert.equal(gaps.length,4);
    for(const [id,c] of gaps){assert.equal(c.status,'passed',id);assert.equal(c.query.method,'slot_trim_witness');assert.ok(c.lower_bound_mm>=.49,id);}
    assert.ok((await page.goto(`${base}/nominal-seating-checks.html`)).ok());
    const text=await page.locator('body').innerText();
    for(const [id] of extraSeats)assert.ok(text.includes(id));
    for(const href of await page.locator('a[href]').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.equal((await page.request.head(href)).status(),200,href);
    await page.screenshot({path:'/tmp/cad-pocketed-mounting-seats.png',fullPage:true});
    assert.ok((await page.goto(`${base}/${output}.preview.html`,{waitUntil:'domcontentloaded'})).ok());
   }else{
    const groups=await json('rigid-translation-groups.json');group=groups.groups.upper_slot_travel;
    assert.equal(group.members.length,29);assert.equal(group.clearance_check_ids.length,4);
    assert.deepEqual(group.translation_world,[6,0,0]);
    for(const id of group.clearance_check_ids){const c=checks.checks[id];assert.equal(c.status,'passed');
     assert.equal(c.query.method,'slot_trim_witness');assert.equal(c.sweep.query_count,2);
     assert.ok(c.sweep.slot_motion_proof);assert.ok(c.lower_bound_mm>=.49);
     assert.equal(c.sweep.slot_motion_proof.stock_slot_wall_faces.length,4);
     assert.ok(c.sweep.slot_motion_proof.complete_path_lower_bound_mm>=.49);
    }
    assert.ok((await page.goto(`${base}/translation-upper_slot_travel.html`,{waitUntil:'domcontentloaded'})).ok());
    await page.waitForFunction(()=>typeof scene!=='undefined'&&scene.instances.length===30&&scene.faces.length>0);
    assert.equal(await page.locator('#translation-inspection').getAttribute('data-error'),null);
    const snapshot=()=>page.evaluate(()=>Object.fromEntries(scene.instances.map(i=>{const f=scene.faces.find(f=>f.instance_id===i.id);return [i.id,{point:f.points[0],normal:f.normal}];})));
    const initial=await snapshot(),moving=new Set(group.members.map(m=>m.instance_id));
    for(const phase of [.125,.5,1]){
     await page.locator('#translation-phase').evaluate((el,value)=>{el.value=String(value);el.dispatchEvent(new Event('input',{bubbles:true}));},phase);
     assert.equal(await page.locator('#translation-inspection').getAttribute('data-phase'),String(phase));
     const after=await snapshot();
     for(const [id,start] of Object.entries(initial)){
      assert.deepEqual(after[id].normal,start.normal);
      for(let k=0;k<3;k++)assert.ok(Math.abs(after[id].point[k]-start.point[k]-(moving.has(id)?phase*group.translation_world[k]:0))<1e-10,id);
     }
    }
    await page.locator('#translation-reset').click();assert.deepEqual(await snapshot(),initial);
   }
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#occurrence-select option').count(),31);
   const selected=[];
   for(const [instance,part] of [[mode==='tightened'?'fixed_base':'adjustment_base','base'],
    [mode==='tightened'?'tight_bolt_0':'adjustment_bolt_0','clamp_bolt_envelope']]){
    const option=await page.locator('#occurrence-select option').evaluateAll((nodes,name)=>nodes.find(n=>n.textContent.includes(name))?.value,instance);
    assert.ok(option,instance);await page.locator('#occurrence-select').selectOption(option);
    const checkSpan=async span=>{assert.equal(span.file,'drive_module_rotation.star');
     await page.waitForFunction(s=>{const el=document.querySelector('#recipe-source');return el?.dataset.sourceFile===s.file&&el?.dataset.startByte===String(s.start_byte);},span);
     assert.equal(await page.locator('#recipe-source-highlight').textContent(),Buffer.from(textFor(span.file)).subarray(span.start_byte,span.end_byte).toString('utf8'));
    };
    await checkSpan(provenance.parts[part].declaration);
    assert.ok((await page.locator('#recipe-summary').textContent()).includes(instance));
    const feature=provenance.part_body_features[part];if(part==='base')assert.equal(feature,'base_deep_relief');
    await page.locator('#recipe-feature-source').click();await checkSpan(provenance.features[feature].declaration);
    assert.equal(await page.locator('#recipe-source').getAttribute('contenteditable'),null);selected.push({instance,part,feature});
   }
   await page.screenshot({path:`/tmp/cad-pocketed-mounting-${mode}.png`});
   await bytes('manifest.json');assert.equal(captured['manifest.json'],manifestSha,'stable artifact generation');
   results.push({mode,recipe_context_hash:manifest.recipe_context_hash,source_bundle_hash:manifest.source_bundle_hash,
    selected,artifacts_sha256:captured,additional_seats:mode==='tightened'?8:0,slot_gap_checks:4,continuous_proofs:group?4:0});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  assert.equal(sha(fs.readFileSync(__filename)),scriptSha);
  console.log(JSON.stringify({valid:true,read_only:true,script_sha256:scriptSha,results,
   screenshots:['/tmp/cad-pocketed-mounting-seats.png','/tmp/cad-pocketed-mounting-tightened.png','/tmp/cad-pocketed-mounting-adjustment.png']},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
