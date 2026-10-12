// Browser acceptance of native paper-mm section sheets; no modeling execution.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1650,height:1200},deviceScaleFactor:1});
  const errors=[],requests=[];page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r.resourceType()));
  const response=await page.goto(`${base}/bearing_drawing/drawing-bearing_sections.svg`);assert.ok(response.ok());
  const evidence=await page.evaluate(()=>{
   const root=document.documentElement;
   return {width:root.getAttribute('width'),height:root.getAttribute('height'),viewBox:root.getAttribute('viewBox'),title:root.querySelector('title').textContent,
    views:[...root.querySelectorAll('svg[id^="drawing-view-"]')].map(view=>{
     const b=view.getBBox(),v=view.viewBox.baseVal;
     return {id:view.id,scale:Number(view.dataset.scale),x:Number(view.getAttribute('x')),y:Number(view.getAttribute('y')),width:Number(view.getAttribute('width')),height:Number(view.getAttribute('height')),viewBox:view.getAttribute('viewBox').split(/\s+/).map(Number),renderedBounds:[b.x,b.y,b.width,b.height],arcs:[...view.querySelectorAll('path')].filter(p=>/A /.test(p.getAttribute('d'))).length,lines:[...view.querySelectorAll('path')].filter(p=>/L /.test(p.getAttribute('d'))).length,dimensions:view.querySelectorAll('.section-dimension,.section-circle-dimension').length};
    }),titleBlock:[...root.children].filter(el=>el.tagName==='text').map(el=>el.textContent)};
  });
  assert.equal(evidence.width,'420mm');assert.equal(evidence.height,'297mm');assert.equal(evidence.viewBox,'0 0 420 297');
  assert.equal(evidence.views.length,2);assert.match(evidence.title,/Bearing fixture/);
  for(const view of evidence.views){
   assert.equal(view.scale,.8);const [x,y,w,h]=view.viewBox,[bx,by,bw,bh]=view.renderedBounds;
   assert.ok(Math.abs(view.width/w-.8)<1e-12&&Math.abs(view.height/h-.8)<1e-12,'native geometry retains declared physical scale');
   assert.ok(view.x>=10&&view.y>=10&&view.x+view.width<=410&&view.y+view.height<=262,'inside paper margins/title block');
   assert.ok(bx>=x-1e-4&&by>=y-1e-4&&bx+bw<=x+w+1e-4&&by+bh<=y+h+1e-4,`rendered annotation overflow: ${JSON.stringify(view)}`);
   assert.ok(view.arcs>0&&view.lines>0&&view.dimensions>0,'native analytic paths and dimension labels present');
  }
  const [a,b]=evidence.views;assert.ok(a.x+a.width<=b.x||b.x+b.width<=a.x||a.y+a.height<=b.y||b.y+b.height<=a.y,'viewports do not overlap');
  assert.ok(evidence.titleBlock.some(t=>t.startsWith('Revision:')));assert.ok(evidence.titleBlock.some(t=>t.startsWith('Source context:')));
  assert.deepEqual(errors,[]);assert.equal(requests.filter(t=>t==='fetch'||t==='xhr').length,0);
  const cdp=await page.context().newCDPSession(page);
  const image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  require('node:fs').writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-bearing-drawing.png',Buffer.from(image.data,'base64'));
  await cdp.detach();
  console.log(JSON.stringify({valid:true,...evidence,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
