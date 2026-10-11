// Independent planar silhouette oracle: top rays must pass through both holes.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const http=require('node:http');
const os=require('node:os');
const {execFileSync}=require('node:child_process');
(async()=>{
 const root=path.resolve(__dirname,'../..'),directory=process.env.CAD_HOLES_DIR||path.join(root,'out/perforated_mount');
 if(!process.env.CAD_HOLES_DIR)execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',path.join(root,'examples/perforated_mount.star'),'--out-dir',directory],{stdio:'pipe'});
 let activeDirectory=directory;
 const temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-hole-refresh-'));
 const plainSource=(await fs.readFile(path.join(root,'examples/perforated_mount.star'),'utf8')).replace(/holes = \[[\s\S]*?\n\]\)/,'holes = [])').replace(/^assert_faces\(.*$/gm,'');
 await fs.writeFile(path.join(temporary,'plain.star'),plainSource);
 execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',path.join(temporary,'plain.star'),'--out-dir',path.join(temporary,'plain')],{stdio:'pipe'});
 const server=http.createServer(async(req,res)=>{try{const name=path.basename(new URL(req.url,'http://localhost').pathname);const bytes=await fs.readFile(path.join(activeDirectory,name));res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(bytes)}catch{res.writeHead(404);res.end()}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:900}}),errors=[],exact=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
  const results=[];
  for(const mode of ['preview','gpu','stream']){
   await page.goto(`http://127.0.0.1:${server.address().port}/perforated_mount.${mode}.html`);
   if(mode!=='preview'){await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.finish()})}
   else await page.locator('[data-view="top"]').click();
   const result=await page.evaluate(mode=>{
    let samples=0,holes=0;const seen=new Set();
    const cap=(part)=>mode==='preview'?scene.faces.find(f=>f.instance_id===part&&f.normal[2]>.999999):null;
    // Sample strictly away from all boundaries; expected regions use only recipe dimensions.
    for(let y=1.37;y<49;y+=2.7)for(let x=1.19;x<79;x+=2.7){
     const lines=[x-15,x-30,y-15,y-35,x-50,x-65,2*(x-50)-3*(y-15),2*(x-50)+3*(y-35),x+y-10,120-x-y,x-y+40,y-x+70];
     if(lines.some(d=>Math.abs(d)<.35))continue;
     const outer=x+y>10&&x-y<70&&y-x<40&&x+y<120;
     const rect=x>15&&x<30&&y>15&&y<35;
     const tri=x>50&&2*(x-50)<3*(y-15)&&2*(x-50)<3*(35-y);
     const cx=137000/3350,locator=Math.abs(x-cx)<2&&Math.abs(y-25)<2;
     if(Math.abs(Math.abs(x-cx)-2)<.3||Math.abs(Math.abs(y-25)-2)<.3)continue;
     const expected=locator?'locator':outer&&!rect&&!tri?'plate':'base';
     let actual,face;
     if(mode==='preview'){
      const scale=Math.min((width-80)/80,(height-150)/50),sx=width/2+(x-40)*scale,sy=height/2+20-(y-25)*scale;
      const id=faceBuffer[Math.floor(sy*renderRatio)*bufferWidth+Math.floor(sx*renderRatio)];face=scene.faces.find(f=>f.id===id);actual=face?.instance_id;
      if(actual===expected&&Math.abs(face.normal[2]-1)>1e-8)throw Error('Picked non-cap face from vertical ray');
     }else{
      const p=cadGpuTest.project([x,y,0]),pick=cadGpuTest.pick(p[0],p[1]);actual=pick?compactScene.instances[pick.instance]?.id:null;
      if(pick)seen.add(`${actual}:${pick.face}`);
     }
     if(actual!==expected)throw Error(`${mode} ray(${x},${y}) expected ${expected}, got ${actual}`);
     if(rect||tri)holes++;samples++;
    }
    if(holes<30)throw Error('Insufficient hole ray coverage');
    return{mode,samples,holeRays:holes,faces:[...seen]};
   },mode);results.push(result);
  }
  await page.goto(`http://127.0.0.1:${server.address().port}/perforated_mount.preview.html`);
  await page.waitForFunction(()=>scene.faces.some(face=>face.triangles));
  activeDirectory=path.join(temporary,'plain');
  await page.waitForFunction(()=>!scene.faces.some(face=>face.triangles),{},{timeout:10000});
  activeDirectory=directory;
  await page.waitForFunction(()=>scene.faces.some(face=>face.triangles),{},{timeout:10000});
  results.push({liveRefresh:'holed→plain→holed',valid:true});
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);console.log(JSON.stringify({valid:true,results,exactGeometryRequests:0},null,2));
 }finally{await browser.close();await new Promise(resolve=>server.close(resolve));await fs.rm(temporary,{recursive:true,force:true})}
})().catch(error=>{console.error(error);process.exitCode=1});
