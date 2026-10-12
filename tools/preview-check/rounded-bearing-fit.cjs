const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/rounded_bearing_fit';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  const manifest=await(await page.request.get(`${base}/manifest.json`)).json();
  const check=manifest.clearance_checks.running_gap;
  assert.equal(check.status,'passed');assert.equal(check.query.method,'bore_trim_witness');
  assert.ok(check.lower_bound_mm<=.15&&check.upper_bound_mm>=.15);
  const source=await(await page.request.get(`${base}/recipe.star`)).text();
  assert.ok((await page.goto(`${base}/clearance-checks.html`)).ok());
  assert.ok((await page.locator('body').textContent()).includes('Shaft in rounded bore'));
  assert.ok((await page.goto(`${base}/rounded_bearing_fit.preview.html`)).ok());
  await page.locator('#recipe-toggle').click();
  assert.equal(await page.locator('#recipe-source').textContent(),source);
  const options=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})));
  for(const name of ['housing','shaft']){
   const option=options.find(o=>o.text.includes(name));assert.ok(option,name);
   await page.locator('#occurrence-select').selectOption(option.value);
   assert.equal(await page.locator('#recipe-source').textContent(),source);
  }
  assert.equal(await page.evaluate(()=>scene.instances.length),2);
  await page.screenshot({path:'/tmp/cad-rounded-bearing-fit.png',fullPage:true});
  assert.ok((await page.goto(`${base}/section-housing_midsection.svg`)).ok());
  assert.equal(await page.locator('g.section-circle-dimension').count(),5);
  assert.equal(Number(await page.locator('[data-id="running_bore_diameter"]').getAttribute('data-value-mm')),12.3);
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,method:check.query.method,source_selection:true,read_only:true,errors},null,2));
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});
