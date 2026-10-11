// Independent analytic ray intersections certify bore openings and inward picking.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const http=require('node:http');
const os=require('node:os');
const {execFileSync}=require('node:child_process');
(async()=>{
 const root=path.resolve(__dirname,'../..'),temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-tube-'));
 const source="p=circle(id='profile',radius=16,inner_radius=10)\nb=extrude(id='ring',profile=p,height=6)\nd=part(id='ring',body=b)\na=assembly(id='ring',instances=[instance(id='ring',part=d)])\nassembly_output(name='ring',assembly=a)\n";
 const sourcePath=path.join(temporary,'ring.star');await fs.writeFile(sourcePath,source);
 const output=path.join(temporary,'out');execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',sourcePath,'--out-dir',output],{stdio:'pipe'});
 const server=http.createServer(async(req,res)=>{try{const name=path.basename(new URL(req.url,'http://localhost').pathname);res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(output,name)))}catch{res.writeHead(404);res.end()}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const errors=[],exact=[],results=[];
 try {
  const page=await browser.newPage({viewport:{width:1200,height:900}});
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
  for(const mode of ['preview','gpu','stream']) {
   await page.goto(`http://127.0.0.1:${server.address().port}/ring.${mode}.html`);
   if(mode!=='preview'){await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('iso');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
   else await page.locator('[data-view="iso"]').click();
   const result=await page.evaluate(({mode})=>{
    const a=mode==='preview'?yaw:-Math.PI/3,b=mode==='preview'?pitch:Math.PI/5;
    const eye=[Math.cos(a)*Math.cos(b),Math.sin(a)*Math.cos(b),Math.sin(b)];
    let samples=0,holes=0;const counts={};
    function oracle(p){
     const hits=[];let uncertain=false;
     for(const [radius,face] of [[16,2],[10,3]]){
      const A=eye[0]**2+eye[1]**2,B=2*(p[0]*eye[0]+p[1]*eye[1]),C=p[0]**2+p[1]**2-radius**2,D=B*B-4*A*C;
      if(Math.abs(D)<3)uncertain=true;
      if(D>0)for(const t of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)]){const z=p[2]+t*eye[2];if(Math.min(Math.abs(z),Math.abs(z-6))<.25)uncertain=true;if(z>0&&z<6)hits.push({t,face})}
     }
     for(const [z,face] of [[0,0],[6,1]]){const t=(z-p[2])/eye[2],r=Math.hypot(p[0]+t*eye[0],p[1]+t*eye[1]);if(Math.min(Math.abs(r-10),Math.abs(r-16))<.25)uncertain=true;if(r>10&&r<16)hits.push({t,face})}
     return {uncertain,hit:hits.sort((x,y)=>y.t-x.t)[0]};
    }
    function pick(p){if(mode!=='preview'){const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?hit.face:null}
     const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
     const project=q=>{q=sub(q,center);return [dot(q,right),-dot(q,up)]};
     const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
     const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p),q=[width/2+r[0]*scale,height/2+20+r[1]*scale];
     if(q[0]<0||q[0]>=width||q[1]<0||q[1]>=height)return null;
     const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)],f=scene.faces.find(f=>f.id===id);return f?f.local_face_id:null;
    }
    for(let x=-19.23;x<20;x+=1.3)for(let y=-19.37;y<20;y+=1.3){const p=[x,y,3],expected=oracle(p);if(expected.uncertain)continue;const face=pick(p);if(face!==(expected.hit?.face??null))throw Error(`${mode} ray${JSON.stringify(p)} expected ${expected.hit?.face}, got ${face}`);if(face===null&&Math.hypot(x,y)<6)holes++;if(face!==null)counts[face]=(counts[face]||0)+1;samples++}
    if(holes<20||!(counts[1]>30&&counts[2]>30&&counts[3]>30))throw Error('Insufficient cap, outer wall, bore wall or through-hole coverage');
    return{mode,samples,throughHoleRays:holes,faceSamples:counts};
   },{mode});results.push(result);
   await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
   if(mode==='stream')await page.screenshot({path:'/tmp/cad-tube-ring.png'});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);console.log(JSON.stringify({results,exactLoads:0,capturedRecipe:true},null,2));
 }finally{await browser.close();await new Promise(r=>server.close(r));await fs.rm(temporary,{recursive:true,force:true})}
})().catch(e=>{console.error(e);process.exitCode=1});
