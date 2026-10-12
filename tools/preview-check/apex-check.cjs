// Independent analytic ray intersections for exterior and interior cone poles.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const http=require('node:http');
const os=require('node:os');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
 const root=path.resolve(__dirname,'../..');
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-apex-'));
 // Each side is radius=a+b*z over [lo,hi], or a planar annulus at z.
 const fixtures=[
  {name:'upper',points:[[0,0],[8,0],[0,12]],height:12,faces:[{z:0,inner:0,outer:8},{a:8,b:-2/3,lo:0,hi:12}]},
  {name:'lower',points:[[0,0],[8,12],[0,12]],height:12,faces:[{a:0,b:2/3,lo:0,hi:12},{z:12,inner:0,outer:8}]},
  {name:'double',points:[[0,0],[8,6],[0,12]],height:12,faces:[{a:0,b:4/3,lo:0,hi:6},{a:16,b:-4/3,lo:6,hi:12}]},
  {name:'blind',points:[[0,0],[10,0],[10,20],[6,20],[0,8]],height:20,faces:[{z:0,inner:0,outer:10},{a:10,b:0,lo:0,hi:20},{z:20,inner:6,outer:10},{a:-4,b:.5,lo:8,hi:20}]},
 ];
 for(const fixture of fixtures){
  fixture.source=`b=revolve(id='body',profile=polygon(id='profile',points=${JSON.stringify(fixture.points)}))\np=part(id='part',body=b)\nassembly_output(name='model',assembly=assembly(id='model',instances=[instance(id='part',part=p)]))\noutput(name='solid',feature=b)\n`;
  const source=path.join(temporary,fixture.name+'.star');await fs.writeFile(source,fixture.source);
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',source,'--out-dir',path.join(temporary,fixture.name)],{stdio:'pipe'});
 }
 server=http.createServer(async(req,res)=>{
  try{const name=new URL(req.url,'http://localhost').pathname;res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(temporary,name)))}
  catch{res.writeHead(404);res.end()}
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1280,height:1000}}),errors=[],exact=[],results=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
 for(const fixture of fixtures)for(const mode of ['preview','gpu','stream']){
  await page.goto(`http://127.0.0.1:${server.address().port}/${fixture.name}/model.${mode}.html`);
  if(mode!=='preview'){await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({fixture,mode})=>{
   const eye=mode==='preview'?[Math.cos(yaw)*Math.cos(pitch),Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch)]:cadGpuTest.snapshot().eye;
   function oracle(p){
    const hits=[];let uncertain=false;
    for(let face=0;face<fixture.faces.length;face++){
     const s=fixture.faces[face];
     if(s.z!==undefined){
      const t=(s.z-p[2])/eye[2],r=Math.hypot(p[0]+t*eye[0],p[1]+t*eye[1]);
      if(Math.abs(r-s.outer)<.35||(s.inner>0&&Math.abs(r-s.inner)<.35))uncertain=true;
      if(r>s.inner&&r<s.outer)hits.push({t,face});
     }else{
      const r=s.a+s.b*p[2],v=s.b*eye[2],A=eye[0]**2+eye[1]**2-v*v;
      const B=2*(p[0]*eye[0]+p[1]*eye[1]-r*v),C=p[0]**2+p[1]**2-r*r,D=B*B-4*A*C;
      if(Math.abs(D)<3)uncertain=true;
      if(D>0)for(const t of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)]){
       const z=p[2]+t*eye[2];
       if(Math.min(Math.abs(z-s.lo),Math.abs(z-s.hi))<.35)uncertain=true;
       if(z>s.lo&&z<s.hi)hits.push({t,face});
      }
     }
    }
    hits.sort((a,b)=>b.t-a.t);return{uncertain,face:hits[0]?.face??null};
   }
   function pick(p){
    if(mode!=='preview'){const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?hit.face:null}
    const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,right),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p),q=[width/2+r[0]*scale,height/2+20+r[1]*scale];
    if(q[0]<0||q[0]>=width||q[1]<0||q[1]>=height)return null;
    const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)],f=scene.faces.find(f=>f.id===id);return f?f.local_face_id:null;
   }
   let samples=0;const counts={};
   for(let x=-12.23;x<13;x+=1.3)for(let y=-12.37;y<13;y+=1.3){
    const p=[x,y,fixture.height/2],expected=oracle(p);if(expected.uncertain)continue;
    const face=pick(p);if(face!==expected.face)throw Error(`${fixture.name}/${mode}: ${JSON.stringify(p)} expected ${expected.face}, got ${face}`);
    samples++;if(face!==null)counts[face]=(counts[face]||0)+1;
   }
   if(samples<100||!Object.keys(counts).length)throw Error('Insufficient apex ray coverage');
   if(fixture.name==='blind'&&!(counts[3]>5))throw Error('Blind conical bore was not inspected');
   return{fixture:fixture.name,mode,samples,faceSamples:counts};
  },{fixture,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),fixture.source);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,exactGeometryDownloads:0,capturedRecipe:true},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
