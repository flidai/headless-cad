// Independent orthographic cylinder silhouette and native-face identity checks.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const http=require('node:http');
const {execFileSync}=require('node:child_process');
(async()=>{
 const root=path.resolve(__dirname,'../..'),directory=path.join(root,'out/shaft_stack');
 execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',path.join(root,'examples/shaft_stack.star'),'--out-dir',directory],{stdio:'pipe'});
 const server=http.createServer(async(req,res)=>{try{const name=path.basename(new URL(req.url,'http://localhost').pathname);res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(directory,name)))}catch{res.writeHead(404);res.end()}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],exact=[],results=[];
 try {
  const page=await browser.newPage({viewport:{width:1200,height:900}});
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
  for(const mode of ['preview','gpu','stream']) {
   await page.goto(`http://127.0.0.1:${server.address().port}/shaft_stack.${mode}.html`);
   if(mode!=='preview'){await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{await cadGpuTest.whenIdle()})}
   for(const view of ['top','front']) {
    if(mode==='preview')await page.locator(`[data-view="${view}"]`).click();
    else await page.evaluate(async(view)=>{cadGpuTest.setView(view);await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()},view);
    const result=await page.evaluate(({mode,view})=>{
     let samples=0;const hitFaces=new Set();
     function pick(p){if(mode!=='preview'){const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?{part:compactScene.instances[hit.instance].part,face:hit.face}:null}
      const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
      const project=q=>{q=sub(q,center);return [dot(q,right),-dot(q,up)]};
      const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
      const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p),q=[width/2+r[0]*scale,height/2+20+r[1]*scale],id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)],f=scene.faces.find(f=>f.id===id);return f?{part:f.part_id,face:f.local_face_id}:null;}
     for(let a=-19.37;a<20;a+=1.7)for(let b=(view==='top'?-19.19:-1.19);b<(view==='top'?20:72);b+=1.7){
      let expected=null,face=view==='top'?1:2,p;
      if(view==='top'){const r=Math.hypot(a,b);if([r-12,r-18].some(v=>Math.abs(v)<.3))continue;expected=r<12?'cap':r<18?'flange':null;p=[a,b,0]}
      else{if([b,b-6,b-66,b-70,Math.abs(a)-8,Math.abs(a)-12,Math.abs(a)-18].some(v=>Math.abs(v)<.3))continue;expected=b>0&&b<6&&Math.abs(a)<18?'flange':b>6&&b<66&&Math.abs(a)<8?'shaft':b>66&&b<70&&Math.abs(a)<12?'cap':null;p=[a,0,b]}
      const hit=pick(p);if((hit?.part??null)!==expected||expected&&hit.face!==face)throw Error(`${mode}/${view} ${JSON.stringify(p)} expected ${expected}/${face}, got ${JSON.stringify(hit)}`);
      if(hit)hitFaces.add(`${hit.part}:${hit.face}`);samples++;
     }
     return {mode,view,samples,faces:[...hitFaces]};
    },{mode,view});results.push(result);
   }
   if(mode==='stream')await page.screenshot({path:'/tmp/cad-shaft-stack.png'});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({results,exactLoads:exact.length},null,2));
 } finally {await browser.close();await new Promise(r=>server.close(r))}
})().catch(e=>{console.error(e);process.exitCode=1});
