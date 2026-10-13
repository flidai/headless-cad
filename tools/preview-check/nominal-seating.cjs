const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  assert.ok((await page.goto(base+'/')).ok());
  const link=page.locator('a[href="/drive_module_rotation/nominal-seating-checks.html"]');
  assert.ok(await link.count()>0);
  await link.first().click();
  assert.equal(await page.locator('h1').innerText(),'Native nominal seating');
  assert.equal(await page.locator('tbody tr').count(),6);
  assert.equal(await page.locator('script').count(),0);
  for(const url of await page.locator('a').evaluateAll(nodes=>nodes.map(n=>n.href)))assert.ok((await page.request.get(url)).ok(),url);
  const dir=base+'/drive_module_rotation';
  const report=await(await page.request.get(dir+'/nominal-seating-checks.json')).json();
  const manifest=await(await page.request.get(dir+'/manifest.json')).json();
  assert.deepEqual(report.checks,manifest.nominal_seating_checks);
  assert.equal(report.recipe_context_hash,manifest.recipe_context_hash);
  const source=await(await page.request.get(dir+'/recipe.star')).text();
  assert.ok(source.includes('nominal_seating_check('));
  for(const [id,check] of Object.entries(report.checks)){
   assert.equal(check.status,'passed');
   assert.ok(check.signed_gap_lower_mm<=0&&check.signed_gap_upper_mm>=0);
   assert.ok(check.disk_radius_lower_mm>=check.declaration.min_shared_disk_radius_mm);
   assert.ok(check.disk_area_lower_mm2>0);
   assert.ok((await page.locator('tbody').innerText()).includes(id));
  }
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  await page.screenshot({path:process.env.CAD_SCREENSHOT||'/tmp/cad-nominal-seating.png'});
  console.log(JSON.stringify({valid:true,checks:6,source_bound:true,read_only:true,page_errors:errors,writes}));
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
