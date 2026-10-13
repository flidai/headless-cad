// Read-only browser acceptance for native check cards in both assembly viewers.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
const crypto=require('node:crypto');

const root=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const scriptSha=sha(fs.readFileSync(__filename));
const cases=[
 {folder:'motor_mount_housing',output:'motor_mount_housing',kinds:{datum_dimension:3}},
 {folder:'seated_pocket_inserts',output:'seated_pocket_inserts',kinds:{datum_dimension:2,nominal_seating:2,pocket_fit:2}},
];
const groups={datum_dimension:['datum-dimensions.json','dimensions',['from','to']],
 nominal_seating:['nominal-seating-checks.json','checks',['left','right']],
 pocket_fit:['pocket-fit-checks.json','checks',['pocket','insert']],
 clearance:['clearance-checks.json','checks',['left','right']]};

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],writes=[],rows=[];
 try{
  const page=await browser.newPage({viewport:{width:1500,height:1000}});
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{if(!['GET','HEAD'].includes(request.method()))writes.push(request.method()+' '+request.url())});
  for(const item of cases){
   const base=`${root}/${item.folder}`;
   const get=async file=>{const response=await page.request.get(`${base}/${file}`);
    assert.equal(response.status(),200,file);return response.json()};
   const manifest=await get('manifest.json');
   const sourceResponse=await page.request.get(`${base}/${manifest.recipe_source.artifact}`);
   assert.equal(sourceResponse.status(),200,'captured root recipe');
   const source=await sourceResponse.text();
   const reports={};for(const [kind,[file]] of Object.entries(groups)){
    if(item.kinds[kind])reports[kind]=await get(file);
   }
   for(const suffix of ['gpu.html','preview.html']){
    const url=`${base}/${item.output}.${suffix}`;
    const response=await page.goto(url,{waitUntil:'domcontentloaded'});
    assert.equal(response.status(),200,url);
    await page.waitForFunction(()=>document.querySelector('#cad-recipe-data')&&document.querySelector('#engineering-checks'));
    const context=await page.locator('#cad-recipe-data').textContent().then(JSON.parse);
    assert.equal(context.output_name,item.output);
    assert.equal(context.content_hash,manifest.outputs[item.output].content_hash);
    assert.equal(context.recipe_context_hash,manifest.recipe_context_hash);
    const checks=context.native_engineering_checks||{};
    const actualKinds={};for(const check of Object.values(checks))actualKinds[check.kind]=(actualKinds[check.kind]||0)+1;
    assert.deepEqual(actualKinds,item.kinds);
    assert.equal(await page.locator('.native-engineering-check').count(),Object.keys(checks).length);
    assert.ok(!(await page.locator('#engineering-checks-summary').innerText()).includes('No engineering checks'));
    for(const [key,check] of Object.entries(checks)){
     const [file,field,endpoints]=groups[check.kind];
     const report=reports[check.kind][field][check.id];
     assert.ok(report,key+' report entry exists');
     const manifestField={datum_dimension:'datum_dimensions',nominal_seating:'nominal_seating_checks',pocket_fit:'pocket_fit_checks',clearance:'clearance_checks'}[check.kind];
     assert.deepEqual(report,manifest[manifestField][check.id],key+' report matches manifest');
     assert.equal(check.report_file,file);
     assert.equal(check.status,report.status);
     assert.equal(check.output,report.declaration.output);
     assert.equal(check.target_content_hash,context.content_hash);
     assert.match(check.report_digest,/^[0-9a-f]{64}$/);
     assert.deepEqual(check.instance_paths,endpoints.map(endpoint=>report[endpoint].instance_path));
     for(const [name,value] of Object.entries(check.quantities))assert.equal(value,report[name],key+' '+name);
     const card=page.locator('.native-engineering-check').filter({has:page.locator(`summary:text-is("${check.id} · ${ {datum_dimension:'Datum dimension',nominal_seating:'Nominal seating',pocket_fit:'Pocket fit',clearance:'Clearance'}[check.kind]} · Passed")`)});
     assert.equal(await card.count(),1,key);
     await card.locator('summary').click();
     assert.equal(await card.getAttribute('data-status'),'passed');
     assert.ok((await card.innerText()).includes('Scope: '+check.scope));
     for(const value of Object.values(check.quantities))assert.ok((await card.innerText()).includes(String(value)));
     assert.equal(await card.locator('a').getAttribute('href'),file);
     assert.equal(await card.locator('button.engineering-source-link').count(),1);
    }
    const first=page.locator('.native-engineering-check').first();
    await first.locator('button.engineering-source-link').click();
    assert.equal(await page.locator('#recipe-panel').isHidden(),false);
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    assert.equal(await page.locator('#recipe-source').getAttribute('aria-label'),'Read-only recipe source');
    assert.equal(await page.locator('#engineering-checks .engineering-check').count(),
     Object.keys(checks).length+Object.keys(context.property_assertions||{}).length);
    rows.push({url,check_count:Object.keys(checks).length,kinds:actualKinds,recipe_context_hash:context.recipe_context_hash});
   }
   if(item.folder==='motor_mount_housing'){
    const partURL=`${base}/housing.preview.html`;
    const response=await page.goto(partURL,{waitUntil:'domcontentloaded'});
    assert.equal(response.status(),200,partURL);
    const part=await page.locator('#cad-recipe-data').textContent().then(JSON.parse);
    assert.equal(part.output_name,'housing');
    assert.equal(part.content_hash,manifest.outputs.housing.content_hash);
    assert.deepEqual(part.native_engineering_checks||{},{});
    assert.equal(await page.locator('.native-engineering-check').count(),0);
   }
  }
  const motionBase=`${root}/drive_module_pocketed_adjustment`;
  const motionManifestResponse=await page.request.get(`${motionBase}/manifest.json`);
  assert.equal(motionManifestResponse.status(),200,'motion preset is published');
  const motionManifest=await motionManifestResponse.json(),motionOutput='drive_module_adjustment';
  const motionReportResponse=await page.request.get(`${motionBase}/clearance-checks.json`);
  assert.equal(motionReportResponse.status(),200,'motion native report');
  const motionReport=await motionReportResponse.json();
  for(const suffix of ['gpu.html','preview.html']){
   const url=`${motionBase}/${motionOutput}.${suffix}`;
   const response=await page.goto(url,{waitUntil:'domcontentloaded'});
   assert.equal(response.status(),200,url);
   const context=await page.locator('#cad-recipe-data').textContent().then(JSON.parse);
   assert.equal(context.content_hash,motionManifest.outputs[motionOutput].content_hash);
   assert.equal(context.recipe_context_hash,motionManifest.recipe_context_hash);
   const checks=Object.values(context.native_engineering_checks||{}).filter(check=>check.kind==='clearance');
   assert.equal(checks.length,4,'four source-declared moving-slot clearances');
   assert.equal(await page.locator('.native-engineering-check[data-kind="clearance"]').count(),4);
   for(const check of checks){
    const report=motionReport.checks[check.id];
    assert.deepEqual(report,motionManifest.clearance_checks[check.id]);
    assert.equal(check.scope,'Entire declared motion path');
    assert.equal(check.report_file,'clearance-checks.json');
    assert.equal(check.output,motionOutput);
    assert.equal(check.target_content_hash,context.content_hash);
    assert.deepEqual(check.instance_paths,[report.left.instance_path,report.right.instance_path]);
    for(const [name,value] of Object.entries(check.quantities))assert.equal(value,report[name],check.id+' '+name);
    const card=page.locator('.native-engineering-check').filter({hasText:check.id+' · Clearance · Passed'});
    assert.equal(await card.count(),1,check.id);
    await card.locator('summary').click();
    assert.ok((await card.innerText()).includes('Scope: Entire declared motion path'));
    assert.equal(await card.locator('a').getAttribute('href'),'clearance-checks.json');
   }
   rows.push({url,check_count:checks.length,scope:'Entire declared motion path'});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({status:'passed',helper_sha256:scriptSha,rows,errors,writes},null,2));
 }finally{await browser.close()}
})().catch(error=>{console.error(error);process.exitCode=1});
