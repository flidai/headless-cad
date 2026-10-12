// Independent box-union/circle ray oracle for a serpentine native opening.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const http=require('node:http');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
 const root=path.resolve(__dirname,'../..');temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-serpentine-'));
 const source=await fs.readFile(path.join(root,'examples/serpentine_manifold.star'),'utf8');
 for(const width of [6,8,10])execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',path.join(root,'examples/serpentine_manifold.star'),'--param',`channel_width=${width}`,'--out-dir',path.join(temporary,String(width))],{stdio:'pipe'});
 server=http.createServer(async(req,res)=>{
  try{const name=new URL(req.url,'http://localhost').pathname;res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(temporary,name)))}catch{res.writeHead(404);res.end()}
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],exact=[],results=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
 for(const w of [6,8,10])for(const mode of ['preview','gpu','stream']){
  await page.goto(`http://127.0.0.1:${server.address().port}/${w}/open_channel.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({w,mode})=>{
   const rects=[[12,12,84,12+w],[84-w,12+w,84,32],[12,32,84,32+w],[12,32+w,12+w,44],[12,44,84,44+w]];
   const bolts=[[6,6],[90,6],[90,58],[6,58]];
   const inside=(x,y,r)=>x>r[0]&&x<r[2]&&y>r[1]&&y<r[3];
   const expected=(x,y)=>bolts.some(([a,b])=>Math.hypot(x-a,y-b)<2.5)?null:rects.some(r=>inside(x,y,r))?'open_floor':'open_core';
   const near=(x,y)=>rects.some(r=>Math.min(Math.abs(x-r[0]),Math.abs(x-r[2]),Math.abs(y-r[1]),Math.abs(y-r[3]))<.4)||bolts.some(([a,b])=>Math.abs(Math.hypot(x-a,y-b)-2.5)<.4);
   function actual(x,y){
    if(mode!=='preview'){
     const q=cadGpuTest.project([x,y,13]),hit=cadGpuTest.pick(q[0],q[1]);
     return hit&&hit.instance>=0?{id:compactScene.instances[hit.instance].id,face:hit.face}:null;
    }
    const scale=zoom*Math.min((width-80)/96,(height-150)/64),px=width/2+(x-48)*scale,py=height/2+20-(y-32)*scale;
    const id=faceBuffer[Math.floor(py*renderRatio)*bufferWidth+Math.floor(px*renderRatio)],face=scene.faces.find(f=>f.id===id);
    return face?{id:face.instance_id,face:face.local_face_id}:null;
   }
   let samples=0;const hits={open_floor:0,open_core:0,miss:0};
   function check(x,y){
    const want=expected(x,y),got=actual(x,y);
    if((got?.id??null)!==want||(got&&got.face!==1))throw Error(`${w}/${mode} at ${x},${y}: expected ${want}/cap1, got ${JSON.stringify(got)}`);
    samples++;hits[want??'miss']++;
   }
   for(let x=2.13;x<94;x+=3.7)for(let y=2.47;y<62;y+=3.1)if(!near(x,y))check(x,y);
   // All three legs, both connectors, the narrow retained web, and bolt voids.
   for(const p of [[48,12+w/2],[84-w/2,26],[48,32+w/2],[12+w/2,38+w/2],[48,44+w/2],[48,43],...bolts])check(...p);
   if(samples<200||hits.open_floor<30||hits.open_core<100||hits.miss<4)throw Error('Insufficient independent coverage');
   return{width:w,mode,samples,hits};
  },{w,mode});results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSource:true,exactDownloads:0},null,2));
})().catch(e=>{console.error(e);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
