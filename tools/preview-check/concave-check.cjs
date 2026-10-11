// Independent top-ray oracle for a nonconvex fork, with and without through holes.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const http=require('node:http');
const os=require('node:os');
const {createHash}=require('node:crypto');
const {execFileSync}=require('node:child_process');
(async()=>{
 const root=path.resolve(__dirname,'../..'),directory=process.env.CAD_CONCAVE_DIR||path.join(root,'out/fork_mount');
 const cad=process.env.CAD_BIN||path.join(root,'target/debug/cad');
 if(!process.env.CAD_CONCAVE_DIR)execFileSync(cad,['build',path.join(root,'examples/fork_mount.star'),'--out-dir',directory],{stdio:'pipe'});
 const temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-concave-'));
 const source=await fs.readFile(path.join(root,'examples/fork_mount.star'),'utf8');
 const plainSource=source.replace(/holes = \[[\s\S]*?\n\]\)/,'holes = [])').replace(/^assert_faces\(id = "opening_walls".*$/gm,'');
 await fs.writeFile(path.join(temporary,'plain.star'),plainSource);
 const plainDirectory=path.join(temporary,'plain');
 execFileSync(cad,['build',path.join(temporary,'plain.star'),'--out-dir',plainDirectory],{stdio:'pipe'});
 let activeDirectory=directory;
 const server=http.createServer(async(req,res)=>{try{const name=path.basename(new URL(req.url,'http://localhost').pathname);const bytes=await fs.readFile(path.join(activeDirectory,name));res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(bytes)}catch{res.writeHead(404);res.end()}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:900}}),errors=[],exact=[],results=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
  for(const holed of [false,true])for(const mode of ['preview','gpu','stream']){
   activeDirectory=holed?directory:plainDirectory;
   await page.goto(`http://127.0.0.1:${server.address().port}/fork_mount.${mode}.html`);
   if(mode!=='preview'){await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
   else await page.locator('[data-view="top"]').click();
   const result=await page.evaluate(({mode,holed})=>{
    let samples=0,bayRays=0,holeRays=0;const faces=new Set();
    // Locator centroid from subtraction of rectangles, independent of kernel tessellation.
    const area=4800-1050-(holed?234:0),cy=(4800*30-1050*42.5-(holed?234*41.5:0))/area-12;
    for(let y=1.37;y<59;y+=2.1)for(let x=1.19;x<79;x+=2.1){
     if([x-25,x-55,y-25,x-8,x-17,x-63,x-72,y-35,y-48,Math.abs(x-40)-2,Math.abs(y-cy)-2].some(d=>Math.abs(d)<.4))continue;
     const bay=x>25&&x<55&&y>25;
     const hole=holed&&((x>8&&x<17)||(x>63&&x<72))&&y>35&&y<48;
     const locator=Math.abs(x-40)<2&&Math.abs(y-cy)<2;
     const expected=locator?'locator':bay||hole?'base':'fork';
     let actual,faceId;
     if(mode==='preview'){
      const scale=Math.min((width-80)/90,(height-150)/70),sx=width/2+(x-40)*scale,sy=height/2+20-(y-30)*scale;
      const id=faceBuffer[Math.floor(sy*renderRatio)*bufferWidth+Math.floor(sx*renderRatio)];
      const face=scene.faces.find(f=>f.id===id);actual=face?.instance_id;faceId=face?.local_face_id;
      if(actual===expected&&Math.abs(face.normal[2]-1)>1e-8)throw Error('Picked non-cap face from top ray');
     }else{
      const p=cadGpuTest.project([x,y,0]),pick=cadGpuTest.pick(p[0],p[1]);actual=pick?compactScene.instances[pick.instance]?.id:null;faceId=pick?.face;
     }
     if(actual!==expected)throw Error(`${mode} holed=${holed} ray(${x},${y}) expected ${expected}, got ${actual}`);
     if(actual==='fork')faces.add(faceId);
     if(bay)bayRays++;if(hole)holeRays++;samples++;
    }
    if(bayRays<100||holed&&holeRays<20||faces.size!==1)throw Error('Missing bay/hole coverage or semantic cap split');
    return{mode,holed,samples,bayRays,holeRays,capFaces:[...faces]};
   },{mode,holed});results.push(result);console.error(`Passed ${mode}, holes=${holed}, ${result.samples} rays`);
  }
  // A verified payload with an unknown envelope version must fail explicitly.
  // Rewrite both bytes and metadata so SHA checking succeeds and reaches schema validation.
  const unknownDirectory=path.join(temporary,'unknown');
  await fs.cp(directory,unknownDirectory,{recursive:true});
  const htmlPath=path.join(unknownDirectory,'fork_mount.stream.html');
  let html=await fs.readFile(htmlPath,'utf8');
  const match=html.match(/<script id="cad-scene-data(?:-v2)?" type="application\/json">([\s\S]*?)<\/script>/);
  assert.ok(match);
  const metadata=JSON.parse(match[1]);
  let changed=false;
  for(const descriptor of Object.values(metadata.mesh_assets)){
   const file=path.join(unknownDirectory,descriptor.url),payload=JSON.parse(await fs.readFile(file,'utf8'));
   if(payload.schema_version===2){
    payload.schema_version=999;const content=JSON.stringify(payload);
    descriptor.byte_length=Buffer.byteLength(content);descriptor.sha256=createHash('sha256').update(content).digest('hex');
    descriptor.url=`mesh-${descriptor.sha256}.display.json`;
    await fs.writeFile(path.join(unknownDirectory,descriptor.url),content);changed=true;break;
   }
  }
  assert.ok(changed);
  html=html.replace(match[1],JSON.stringify(metadata));await fs.writeFile(htmlPath,html);
  activeDirectory=unknownDirectory;
  await page.goto(`http://127.0.0.1:${server.address().port}/fork_mount.stream.html`);
  await page.waitForFunction(()=>window.cadGpuTest);
  await page.evaluate(async()=>{await cadGpuTest.whenIdle()});
  const failure=await page.evaluate(()=>JSON.stringify(cadGpuStats));
  assert.match(failure,/Unsupported display mesh schema/);
  results.push({unknownDisplaySchema:'rejected after successful digest verification'});
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({valid:true,results,exactGeometryRequests:0},null,2));
 }finally{await browser.close();await new Promise(resolve=>server.close(resolve));await fs.rm(temporary,{recursive:true,force:true})}
})().catch(error=>{console.error(error);process.exitCode=1});
