// Inspect artifacts delivered by the MCP client; no modeling calls from the browser.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_DEMO_URL||'http://localhost:8080/agent_recovery_demo').replace(/\/$/,'');
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/index.html`)).ok());
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href))) assert.ok((await page.request.get(link)).ok(),link);
  const report=await(await page.request.get(`${base}/evidence.json`)).json();
  assert.equal(report.valid,true);assert.equal(report.source_workspace_unchanged,true);
  assert.equal(report.durable_jobs,3);assert.equal(report.recovered_retry_results,3);
  assert.equal(report.rejected_job.worker.success,false);
  assert.equal(report.conflicting_retry.code,'agent_request_failed');
  if (report.acceptances?.length) {
   assert.equal(report.acceptances.length,2);
   const [a,b]=report.acceptances;
   assert.equal(a.accepted,true);assert.equal(b.accepted,true);
   assert.equal(a.revision.sequence,1);assert.equal(b.revision.sequence,2);
   assert.equal(b.revision.expected_base_revision,a.revision.revision_id);
   assert.equal(report.stale_acceptance.code,'agent_request_failed');
   assert.match(await page.locator('body').innerText(),/stale proposal was refused/);
  }
  if (report.deliveries?.some(d=>d.mode==='verified_generation_stream')) {
   assert.equal(report.deliveries.length,2);
   assert.ok(report.deliveries.every(d=>d.mode==='verified_generation_stream' && d.bytes>0 && d.read_calls>0));
   assert.match(await page.locator('body').innerText(),/Both delivery streams passed a final integrity check/);
  }
  if (report.offline_replay) {
   const replay=report.offline_replay;
   assert.equal(replay.valid,true);
   assert.equal(replay.accepted_revision_id,report.acceptances[1].revision.revision_id);
   assert.equal(replay.accepted_manifest_digest,report.acceptances[1].revision.manifest_digest);
   assert.ok(replay.compared_files.some(f=>f.name.endsWith('.brep.json')));
   assert.ok(replay.compared_files.some(f=>f.name.endsWith('.step')));
   assert.match(await page.locator('body').innerText(),/Offline replay of accepted revision B/);
  }
  await page.screenshot({path:'/tmp/cad-agent-recovery-demo.png',fullPage:true});
  for(const [name,diameter] of [['before',12],['after',14]]) {
   const source=await(await page.request.get(`${base}/${name}/recipe.star`)).text();
   const manifest=await(await page.request.get(`${base}/${name}/manifest.json`)).json();
   const fixture=report.mechanical_fixture?.[name];
   const [occurrences,definitions]=fixture?[30,14]:[24,11];
   assert.equal(manifest.outputs.drive_module_release.instances,occurrences);
   assert.equal(manifest.outputs.drive_module_release.definitions,definitions);
   if(fixture){
    assert.equal(fixture.occurrences,30);assert.equal(fixture.definitions,14);
    assert.deepEqual(fixture.face_attached_pilot_patterns,
     ['housing_left_pilots','housing_right_pilots']);
    assert.ok(source.includes('hole_pattern(id = feature_id'));
    assert.ok(source.includes('id = "end_cover_body"'));
    assert.ok(source.includes('id = "cover_screw_body"'));
   }
   const expectedChecks=source.includes('left_cover_shaft_gap')?13:11;
   assert.equal(Object.keys(manifest.clearance_checks).length,expectedChecks);
   if(fixture)assert.equal(fixture.named_continuous_clearance_checks,expectedChecks);
   if(fixture){
    const tableSource=source.includes('id = "end_cover_machining"');
    assert.equal(Object.keys(manifest.datum_dimensions).length,tableSource?20:10);
    assert.equal(fixture.native_axial_dimensions,tableSource?20:10);
    if(tableSource){
     assert.equal(fixture.native_machining_dimension_rows,10);
     assert.equal(manifest.drawing_sheets.end_cover_machining.status,'published');
    }
   }
   assert.ok((await page.goto(`${base}/${name}/rotation-rotor.html`)).ok());
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-error'),null);
   await page.locator('#recipe-toggle').click();
   const parts=['shaft','parallel_key','shaft_ring'];
   if(fixture)parts.push('left_cover','right_cover_screw_plus');
   for(const part of parts) {
    const id=await page.locator('#occurrence-select option').evaluateAll((nodes,part)=>nodes.find(n=>n.textContent.includes(part))?.value,part);
    assert.ok(id);await page.locator('#occurrence-select').selectOption(id);
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    assert.ok(source.includes(`param(name = "shaft_diameter", default = ${diameter})`));
   }
   await page.locator('#rotation-phase').evaluate(slider=>{slider.value='.25';slider.dispatchEvent(new Event('input',{bubbles:true}));});
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),'0.25');
  }
  await page.screenshot({path:'/tmp/cad-agent-recovery-result.png',fullPage:true});
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,source_revisions:[12,14],read_only:true,errors},null,2));
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});
