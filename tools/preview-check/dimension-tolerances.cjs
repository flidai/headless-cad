// Independent physical limits and native SVG layout acceptance.
const {chromium}=require('playwright'),assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,''),dir=`${base}/bearing_limits`;
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:900},deviceScaleFactor:1});
  const get=async name=>{const r=await page.request.get(`${dir}/${name}`);assert.ok(r.ok());return r.json();};
  const report=await get('section-pilot_limits.json'),view=report.section_view;
  const dimensions=[...view.dimensions,...view.circle_dimensions];
  const expected={bore_size:[6,0,.02,6,6.02],pin_size:[5.6,-.02,0,5.58,5.6],station_pitch:[90,-.05,.05,89.95,90.05]};
  for(const [id,values] of Object.entries(expected)){
   const d=dimensions.find(d=>d.declaration.id===id);assert.ok(d);assert.equal(d.status,'passed');assert.ok(Math.abs(d.value_mm-values[0])<1e-9);
   for(const [i,key] of ['nominal_mm','lower_deviation_mm','upper_deviation_mm','lower_limit_mm','upper_limit_mm'].entries())assert.ok(Math.abs(d.tolerance[key]-values[i])<1e-12,`${id} ${key}`);
  }
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const response=await page.goto(`${dir}/drawing-bearing_limits.svg`);assert.ok(response.ok());
  const evidence=await page.evaluate(()=>{
   const root=document.documentElement,view=root.querySelector('svg[id^="drawing-view-"]'),b=view.getBBox();
   return {width:root.getAttribute('width'),height:root.getAttribute('height'),paper:root.getAttribute('viewBox'),scale:Number(view.dataset.scale),widthMm:Number(view.getAttribute('width')),heightMm:Number(view.getAttribute('height')),viewBox:view.getAttribute('viewBox').split(/\s+/).map(Number),bounds:[b.x,b.y,b.width,b.height],labels:[...view.querySelectorAll('.section-dimension text,.section-circle-dimension text')].map(e=>e.textContent),paths:[...view.querySelectorAll('path[id]')].map(e=>e.getAttribute('d'))};
  });
  assert.equal(evidence.width,'297mm');assert.equal(evidence.height,'210mm');assert.equal(evidence.paper,'0 0 297 210');assert.equal(evidence.scale,1);
  for(const label of ['bore_size: 6 +0.02/+0 mm diameter','pin_size: 5.6 +0/-0.02 mm diameter','station_pitch: 90 +0.05/-0.05 mm'])assert.ok(evidence.labels.includes(label),label);
  const [x,y,w,h]=evidence.viewBox,[bx,by,bw,bh]=evidence.bounds;
  assert.equal(evidence.widthMm,w);assert.equal(evidence.heightMm,h);
  assert.ok(bx>=x-1e-4&&by>=y-1e-4&&bx+bw<=x+w+1e-4&&by+bh<=y+h+1e-4,'text and geometry fit canvas');
  assert.ok(evidence.paths.some(p=>p.includes('L '))&&evidence.paths.some(p=>p.includes('A ')),'native planar/circular shapes present');
  // Sheet rendering must retain the source section paths byte-for-byte.
  const sectionResponse=await page.request.get(`${dir}/section-pilot_limits.svg`);assert.ok(sectionResponse.ok());
  const section=await sectionResponse.text();
  const sourcePaths=[...section.matchAll(/<path id="[^"]+"[^>]* d="([^"]*)"/g)].map(m=>m[1]);
  assert.deepEqual(evidence.paths,sourcePaths);
  assert.deepEqual(errors,[]);
  const cdp=await page.context().newCDPSession(page),image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  fs.writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-bearing-limits.png',Buffer.from(image.data,'base64'));await cdp.detach();
  console.log(JSON.stringify({valid:true,expected_limits:expected,...evidence,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
