const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/rounded_bearing_cover';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try {
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  const manifest=await(await page.request.get(`${base}/manifest.json`)).json();
  const expected=(2400-(4-Math.PI)*16-Math.PI*125)*8;
  assert.ok(Math.abs(manifest.outputs.bearing_cover.volume_mm3-expected)<1e-7);
  const source=await(await page.request.get(`${base}/recipe.star`)).text();
  assert.ok(source.includes('fillet('));
  for(const name of ['rounded_blank','bearing_cover']) {
   assert.ok((await page.goto(`${base}/${name}.preview.html`)).ok());
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#recipe-source').textContent(),source);
   assert.ok((await page.evaluate(()=>scene.faces.length))>=10);
   const rect=await page.locator('canvas').first().boundingBox();assert.ok(rect);
   await page.mouse.click(rect.x+rect.width*.5,rect.y+rect.height*.5);
  }
  await page.screenshot({path:'/tmp/cad-longitudinal-fillet.png',fullPage:true});
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,volume_mm3:expected,read_only:true,errors},null,2));
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});
