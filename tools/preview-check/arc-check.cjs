// Independent plane/circle ray oracle for trimmed line/arc prisms.
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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-arc-'));
 const line=end=>({kind:'line_to',end});
 const arc=(center,end)=>({kind:'arc_to',center,end,clockwise:false});
 const shapes=[
  {name:'capsule',start:[-10,-5],segments:[line([10,-5]),arc([10,0],[10,5]),line([-10,5]),arc([-10,0],[-10,-5])],hole:[0,0],radius:5,extent:[17,8]},
  {name:'rounded',start:[-16,-15],segments:[line([16,-15]),arc([16,-11],[20,-11]),line([20,11]),arc([16,11],[16,15]),line([-16,15]),arc([-16,11],[-20,11]),line([-20,-11]),arc([-16,-11],[-16,-15])],hole:[7,3],radius:4,extent:[23,18]},
 ];
 const fixtures=[];
 for(const shape of shapes)for(const oblique of [false,true]){
  const fixture={...shape,name:shape.name+(oblique?'_oblique':''),oblique};
  const segments=JSON.stringify(shape.segments).replaceAll('false','False');
  fixture.source=`p=path(id='profile',start=${JSON.stringify(shape.start)},segments=${segments},circular_holes=[{'center':${JSON.stringify(shape.hole)},'radius':2}])\nb=extrude(id='body',profile=p,height=8)\np=part(id='part',body=b)\nassembly_output(name='model',assembly=assembly(id='model',instances=[instance(id='part',part=p${oblique?',rotation=[0.5,0.5,0,0.7071067811865476],offset=[12,-7,4]':''})]))\n`;
  const source=path.join(temporary,fixture.name+'.star');await fs.writeFile(source,fixture.source);
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',source,'--out-dir',path.join(temporary,fixture.name)],{stdio:'pipe'});
  fixtures.push(fixture);
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
   const q=Math.SQRT1_2,R=fixture.oblique?[[.5,.5,q],[.5,.5,-q],[-q,q,0]]:[[1,0,0],[0,1,0],[0,0,1]],offset=fixture.oblique?[12,-7,4]:[0,0,0];
   const world=p=>R.map((row,i)=>row.reduce((s,v,j)=>s+v*p[j],offset[i]));
   const worldEye=mode==='preview'?[Math.cos(yaw)*Math.cos(pitch),Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch)]:cadGpuTest.snapshot().eye;
   const eye=[0,1,2].map(i=>R.reduce((s,row,j)=>s+row[i]*worldEye[j],0));
   const capsule=fixture.name.startsWith('capsule');
   const distance=(x,y)=>capsule?Math.hypot(Math.max(Math.abs(x)-10,0),y)-5:Math.hypot(Math.max(Math.abs(x)-16,0),Math.max(Math.abs(y)-11,0))-4;
   const holeDistance=(x,y)=>Math.hypot(x-fixture.hole[0],y-fixture.hole[1])-2;
   const cross=(a,b)=>a[0]*b[1]-a[1]*b[0];
   const walls=[];let start=fixture.start;
   for(const s of fixture.segments){walls.push({...s,start});start=s.end}
   function oracle(p){
    const hits=[];let uncertain=false;
    for(const [face,z] of [[0,0],[1,8]])if(Math.abs(eye[2])>1e-10){
     const t=(z-p[2])/eye[2],x=p[0]+t*eye[0],y=p[1]+t*eye[1],d=distance(x,y),h=holeDistance(x,y);
     if(Math.abs(d)<.28||Math.abs(h)<.28)uncertain=true;
     if(d<0&&h>0)hits.push({t,face});
    }
    walls.forEach((s,i)=>{
     if(s.kind==='line_to'){
      const v=[s.end[0]-s.start[0],s.end[1]-s.start[1]],den=cross(v,eye);
      if(Math.abs(den)<1e-10)return;
      const t=-cross(v,[p[0]-s.start[0],p[1]-s.start[1]])/den;
      const point=[p[0]+t*eye[0],p[1]+t*eye[1]],z=p[2]+t*eye[2];
      const u=((point[0]-s.start[0])*v[0]+(point[1]-s.start[1])*v[1])/(v[0]**2+v[1]**2);
      if(u>=0&&u<=1&&z>=0&&z<=8){if(Math.min(u,1-u)*Math.hypot(...v)<.28||Math.min(z,8-z)<.28)uncertain=true;hits.push({t,face:i+2})}
     }else circle(s.center,fixture.radius,i+2,s);
    });
    circle(fixture.hole,2,walls.length+2,null);
    function circle(center,radius,face,segment){
     const dx=p[0]-center[0],dy=p[1]-center[1],A=eye[0]**2+eye[1]**2,B=2*(dx*eye[0]+dy*eye[1]),C=dx*dx+dy*dy-radius**2,D=B*B-4*A*C;
     if(Math.abs(D)<1)uncertain=true;
     if(D<=0||A<1e-10)return;
     for(const t of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)]){
      const x=p[0]+t*eye[0],y=p[1]+t*eye[1],z=p[2]+t*eye[2];
      let onArc=true;
      if(segment){
       const angle=Math.atan2(y-center[1],x-center[0]),a=Math.atan2(segment.start[1]-center[1],segment.start[0]-center[0]),b=Math.atan2(segment.end[1]-center[1],segment.end[0]-center[0]);
       const wrap=v=>(v+2*Math.PI)%(2*Math.PI),u=wrap(angle-a),span=wrap(b-a);
       onArc=u<=span+1e-10;
       if(onArc&&Math.min(u,span-u)*radius<.28&&z>=0&&z<=8)uncertain=true;
      }
      if(onArc&&z>=0&&z<=8){if(Math.min(z,8-z)<.28)uncertain=true;hits.push({t,face})}
     }
    }
    hits.sort((a,b)=>b.t-a.t);return{uncertain,face:hits[0]?.face??null};
   }
   function pick(local){
    const p=world(local);
    if(mode!=='preview'){const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?hit.face:null}
    const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,right),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p),q=[width/2+r[0]*scale,height/2+20+r[1]*scale];
    if(q[0]<0||q[0]>=width||q[1]<0||q[1]>=height)return null;
    const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)],f=scene.faces.find(f=>f.id===id);return f?f.local_face_id:null;
   }
   let samples=0,misses=0;const counts={};
   for(let x=-fixture.extent[0]+.23;x<fixture.extent[0];x+=1.1)for(let y=-fixture.extent[1]+.37;y<fixture.extent[1];y+=1.1){
    const p=[x,y,4],expected=oracle(p);if(expected.uncertain)continue;
    const face=pick(p);if(face!==expected.face)throw Error(`${fixture.name}/${mode}: ${JSON.stringify(p)} expected ${expected.face}, got ${face}`);
    samples++;if(face!==null)counts[face]=(counts[face]||0)+1;else misses++;
   }
   const curved=walls.reduce((n,s,i)=>n+(s.kind==='arc_to'?(counts[i+2]||0):0),0);
   if(samples<100||curved<5||misses<5)throw Error('Insufficient arc/cap/miss coverage '+JSON.stringify({samples,curved,misses,counts}));
   if((counts[walls.length+2]||0)<5)throw Error('Circular bore was not inspected');
   return{fixture:fixture.name,mode,samples,curved,misses,faceSamples:counts};
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
