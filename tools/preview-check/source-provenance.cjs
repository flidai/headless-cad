// Verify read-only source navigation against captured declaration byte spans.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const crypto=require('node:crypto');
const fs=require('node:fs');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_adjustment';
const moduleText=(bundle,label)=>label===bundle.source_name?bundle.source:bundle.modules[label];
const spanText=(bundle,span)=>Buffer.from(moduleText(bundle,span.file),'utf8').subarray(span.start_byte,span.end_byte).toString('utf8');

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1400,height:950}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(r.method()!=='GET')writes.push(`${r.method()} ${r.url()}`);});
  const bundle=await(await page.request.get(`${base}/recipe.bundle.json`)).json();
  const provenanceResponse=await page.request.get(`${base}/source-provenance.json`);
  const provenanceBytes=await provenanceResponse.body();
  const provenance=JSON.parse(provenanceBytes.toString('utf8'));
  const manifest=await(await page.request.get(`${base}/manifest.json`)).json();
  assert.equal(provenance.recipe_context_hash,manifest.recipe_context_hash);
  assert.equal(provenance.source_bundle_hash,manifest.source_bundle_hash);
  assert.equal(manifest.source_provenance.artifact,'source-provenance.json');
  for(const [label,item] of Object.entries(provenance.source_inventory)){
   const bytes=Buffer.from(moduleText(bundle,label),'utf8');
   assert.equal(item.bytes,bytes.length);
   assert.equal(item.sha256,crypto.createHash('sha256').update(bytes).digest('hex'));
  }
  assert.ok((await page.goto(`${base}/translation-upper_slot_travel.html`)).ok());
  assert.equal(await page.locator('#translation-inspection').getAttribute('data-error'),null);
  await page.locator('#recipe-toggle').click();
  assert.equal(await page.locator('#recipe-declarations').count(),1);

  async function select(name,part){
   const option=(await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})))).find(o=>o.text.includes(name));
   assert.ok(option,`selectable ${name}`);
   await page.locator('#occurrence-select').selectOption(option.value);
   const source=provenance.parts[part].declaration;
   await page.waitForFunction(span=>{
    const node=document.querySelector('#recipe-source');
    return node?.dataset.sourceFile===span.file&&
     node?.dataset.startByte===String(span.start_byte)&&
     node?.dataset.endByte===String(span.end_byte);
   },source);
   assert.equal(await page.locator('#recipe-module').inputValue(),source.file);
   assert.equal(await page.locator('#recipe-source').textContent(),moduleText(bundle,source.file));
   assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,source));
   assert.match(await page.locator('#recipe-source-location').textContent(),new RegExp(source.file.replace(/[.*+?^${}()|[\]\\]/g,'\\$&')));
   assert.ok((await page.locator('#recipe-source-location').textContent()).includes(String(source.start_line)));
   assert.ok((await page.locator('#recipe-summary').textContent()).includes(name));
   assert.equal(await page.locator('#recipe-part-source').getAttribute('data-source-role'),'part');
   return {option,part,source};
  }

  const shaft=await select('shaft','shaft');
  assert.equal(shaft.source.file,'drive_module_rotation.star');
  const bodyFeature=provenance.part_body_features.shaft;
  assert.ok(provenance.features[bodyFeature]);
  await page.locator('#recipe-feature-source').click();
  const feature=provenance.features[bodyFeature].declaration;
  await page.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),feature);
  assert.equal(await page.locator('#recipe-module').inputValue(),feature.file);
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,feature));
  assert.equal(await page.locator('#recipe-source').textContent(),moduleText(bundle,feature.file));

  // Selecting a different occurrence automatically follows its defining part.
  const baseSelection=await select('adjustment_base','base');
  assert.equal(baseSelection.source.file,'drive_module_rotation.star');
  const bolt=await select('adjustment_bolt_0','clamp_bolt_envelope');
  assert.equal(bolt.source.file,'drive_module_rotation.star');
  await page.locator('#translation-phase').evaluate(slider=>{
   slider.value='.5';slider.dispatchEvent(new Event('input',{bubbles:true}));
  });
  assert.equal(await page.locator('#translation-inspection').getAttribute('data-phase'),'0.5');
  assert.equal(await page.locator('#recipe-source-highlight').textContent(),spanText(bundle,bolt.source));
  assert.equal(await page.locator('#recipe-source').textContent(),moduleText(bundle,bolt.source.file));
  assert.equal(await page.locator('.badge').textContent(),'READ-ONLY MOTION');

  // The source picker exposes the full captured root, and manual navigation
  // does not change the selected part or initiate model execution.
  await page.locator('#recipe-module').selectOption('');
  assert.equal(await page.locator('#recipe-source').textContent(),bundle.source);
  assert.ok((await page.locator('#recipe-summary').textContent()).includes('adjustment_bolt_0'));
  const tamper=await page.evaluate(async()=>{
   const copied=structuredClone(recipeContext),label='drive_module_rotation.star';
   const original=copied.modules[label];
   copied.modules[label]=original.replace('shaft = part','shuft = part');
   if(copied.modules[label]===original||new TextEncoder().encode(copied.modules[label]).length!==new TextEncoder().encode(original).length)throw Error('tamper setup failed');
   const host=document.createElement('div'),picker=document.createElement('select'),source=document.createElement('pre');
   host.append(picker,source);document.body.append(host);
   try{
    const navigator=new CadSourceNavigation({picker,source});
    await Promise.resolve(navigator.render(copied,{instance_id:'tamper',part_id:'shaft',feature_id:'shaft_body'}));
    await new Promise(resolve=>setTimeout(resolve,300));
    return {hidden:navigator.navigation.hidden,mark:source.querySelector('mark')?.textContent||null,
     location:navigator.location.textContent,source:source.textContent};
   }finally{host.remove();}
  });
  assert.equal(tamper.hidden,true,'same-byte-length source substitution refuses declaration navigation');
  assert.equal(tamper.mark,null,'tampered source has no misleading highlight');
  assert.ok(tamper.location.toLowerCase().includes('mismatch')||tamper.location.toLowerCase().includes('digest'),
   `tamper diagnostic: ${tamper.location}`);
  const manualRace=await page.evaluate(async()=>{
   const copied=structuredClone(recipeContext),host=document.createElement('div');
   const picker=document.createElement('select'),source=document.createElement('pre');
   host.append(picker,source);document.body.append(host);
   const subtle=crypto.subtle,original=subtle.digest.bind(subtle);
   Object.defineProperty(subtle,'digest',{configurable:true,value:async(...args)=>{
    await new Promise(resolve=>setTimeout(resolve,75));return original(...args);
   }});
   try{
    const navigator=new CadSourceNavigation({picker,source});
    const pending=navigator.render(copied,{instance_id:'race',part_id:'shaft',feature_id:'shaft_body'});
    picker.value='';picker.dispatchEvent(new Event('change',{bubbles:true}));
    await pending;
    return {selected:picker.value,wholeSource:source.textContent===copied.source,
     highlighted:Boolean(source.querySelector('mark')),buttons:navigator.navigation.querySelectorAll('button').length};
   }finally{delete subtle.digest;host.remove();}
  });
  assert.equal(manualRace.selected,'');
  assert.equal(manualRace.wholeSource,true,'manual root source selection survives digest verification');
  assert.equal(manualRace.highlighted,false,'late digest result does not replace manual full-source view');
  assert.ok(manualRace.buttons>=2,'validated declaration navigation remains available');
  const gpu=await browser.newPage({viewport:{width:1400,height:950}});
  gpu.on('pageerror',e=>errors.push(`GPU: ${e.message}`));
  gpu.on('request',r=>{if(r.method()!=='GET')writes.push(`${r.method()} ${r.url()}`);});
  assert.ok((await gpu.goto(`${base}/drive_module_adjustment.gpu.html`)).ok());
  await gpu.waitForFunction(()=>window.cadGpuStats?.ready===true);
  await gpu.locator('#recipe-toggle').click();
  const gpuIndex=await gpu.evaluate(()=>compactScene.instances.findIndex(item=>item.part==='shaft'));
  assert.ok(gpuIndex>=0,'GPU scene contains shaft part');
  await gpu.evaluate(index=>cadGpuTest.select(index),gpuIndex);
  await gpu.waitForFunction(span=>document.querySelector('#recipe-source')?.dataset.startByte===String(span.start_byte),shaft.source);
  assert.equal(await gpu.locator('#recipe-module').inputValue(),shaft.source.file);
  assert.equal(await gpu.locator('#recipe-source').textContent(),moduleText(bundle,shaft.source.file));
  assert.equal(await gpu.locator('#recipe-source-highlight').textContent(),spanText(bundle,shaft.source));
  assert.equal(await gpu.locator('#recipe-feature-source').count(),1);
  await gpu.close();

  const paged=await browser.newPage({viewport:{width:1400,height:950}});
  paged.on('pageerror',e=>errors.push(`Progressive: ${e.message}`));
  paged.on('request',r=>{if(r.method()!=='GET')writes.push(`${r.method()} ${r.url()}`);});
  assert.ok((await paged.goto(`${base}/drive_module_adjustment.paged.html`)).ok());
  await paged.waitForFunction(()=>window.cadPagedStats?.ready===true);
  await paged.evaluate(()=>cadPaged.whenIdle());
  const pagedIndex=await paged.evaluate(()=>cadPaged.snapshot().metadata.entries.find(entry=>entry.instance.part==='shaft')?.index??null);
  assert.ok(Number.isInteger(pagedIndex),'progressive metadata contains shaft part');
  await paged.evaluate(index=>cadPaged.select(index),pagedIndex);
  await paged.waitForFunction(span=>document.querySelector('#source')?.dataset.startByte===String(span.start_byte),shaft.source);
  assert.equal(await paged.locator('#module').inputValue(),shaft.source.file);
  assert.equal(await paged.locator('#source').textContent(),moduleText(bundle,shaft.source.file));
  assert.equal(await paged.locator('#recipe-source-highlight').textContent(),spanText(bundle,shaft.source));
  assert.equal(await paged.locator('#recipe-feature-source').count(),1);
  await paged.close();
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  const report={valid:true,part_declarations_checked:3,feature_jump:bodyFeature,
   imported_module:'drive_module_rotation.star',motion_phase_preserved:.5,
   equal_length_source_tamper_refused:true,manual_selection_race_preserved:true,
   gpu_part_navigation:true,progressive_part_navigation:true,
   source_bundle_hash:provenance.source_bundle_hash,read_only:true,
   source_provenance_sha256:crypto.createHash('sha256').update(provenanceBytes).digest('hex'),
   browser_script_sha256:crypto.createHash('sha256').update(fs.readFileSync(__filename)).digest('hex')};
  console.log(JSON.stringify(report,null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
