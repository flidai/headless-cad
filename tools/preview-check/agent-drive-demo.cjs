// Inspect artifacts delivered by the MCP client; no modeling calls from the browser.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_DEMO_URL||'http://localhost:8080/agent_drive_demo').replace(/\/$/,'');
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(`${base}/index.html`)).ok());
  for(const link of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href))) assert.ok((await page.request.get(link)).ok(),link);
  const report=await(await page.request.get(`${base}/report.json`)).json();
  assert.equal(report.valid,true);assert.equal(report.workspace_unchanged,true);assert.equal(report.offline_replay_native_and_step_equal,true);
  assert.equal(report.jobs.length,3);assert.equal(report.native_rejection.code,'clearance_check_failed');
  await page.screenshot({path:'/tmp/cad-agent-drive-demo.png',fullPage:true});
  for(const [name,diameter] of [['before',12],['after',14]]) {
   const source=await(await page.request.get(`${base}/${name}/recipe.star`)).text();
   const manifest=await(await page.request.get(`${base}/${name}/manifest.json`)).json();
   const covers=source.includes('id = "end_cover_body"');
   assert.equal(manifest.outputs.drive_module_release.instances,covers?30:24);
   assert.equal(manifest.outputs.drive_module_release.definitions,covers?14:11);
   assert.equal(Object.keys(manifest.clearance_checks).length,11);
   assert.ok((await page.goto(`${base}/${name}/rotation-rotor.html`)).ok());
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-error'),null);
   await page.locator('#recipe-toggle').click();
   const parts=['shaft','parallel_key','shaft_ring'];
   if(covers)parts.push('left_cover','right_cover_screw_plus');
   for(const part of parts) {
    const id=await page.locator('#occurrence-select option').evaluateAll((nodes,part)=>nodes.find(n=>n.textContent.includes(part))?.value,part);
    assert.ok(id);await page.locator('#occurrence-select').selectOption(id);
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    assert.ok(source.includes(`param(name = "shaft_diameter", default = ${diameter})`));
   }
   await page.locator('#rotation-phase').evaluate(slider=>{slider.value='.25';slider.dispatchEvent(new Event('input',{bubbles:true}));});
   assert.equal(await page.locator('#rotation-inspection').getAttribute('data-phase'),'0.25');
  }
  await page.screenshot({path:'/tmp/cad-agent-drive-result.png',fullPage:true});
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,source_revisions:[12,14],read_only:true,errors},null,2));
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});
