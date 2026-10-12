// Annular shoulders, outer caps and through openings must remain distinct in every viewer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path'),http=require('node:http'),os=require('node:os');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
 const root=path.resolve(__dirname,'../..'),filename=path.join(root,'examples/counterbored_fixture.star'),source=await fs.readFile(filename,'utf8');
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-counterbore-'));
 const fixtures=[[7,6,21],[6,4,18],[8,8,24]];
 for(const [i,dims] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(i))];
  dims.forEach((n,j)=>args.push('--param',`${['counterbore_radius','counterbore_depth','plate_height'][j]}=${n}`));
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{stdio:'pipe'});
 }
 server=http.createServer(async(req,res)=>{try{const name=new URL(req.url,'http://localhost').pathname;res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(temporary,name)));}catch{res.writeHead(404);res.end();}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],exact=[],results=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url());});
 for(const [i,dims] of fixtures.entries())for(const mode of ['preview','gpu','stream'])for(const side of ['top','bottom']){
  await page.goto(`http://127.0.0.1:${server.address().port}/${i}/plate_inspection.${mode}.html`);
  if(mode!=='preview')await page.waitForFunction(()=>window.cadGpuTest);
  const native=JSON.parse(await fs.readFile(path.join(temporary,String(i),'plate.brep.json'),'utf8')).model;
  const centers=[[20,20],[60,20],[100,20],[20,60],[60,60],[100,60]];
  const floorIds=centers.map(([x,y])=>native.faces.findIndex(f=>f.surface.kind==='plane'&&f.loops.length===2&&Math.abs(f.surface.geometry.origin[2]-(side==='top'?dims[2]-dims[1]:3))<1e-7&&f.loops.some(l=>native.loops[l].edges.some(e=>{const curve=native.edges[e.edge].curve;return curve.kind==='circle'&&Math.abs(curve.geometry.origin[0]-x)<1e-7&&Math.abs(curve.geometry.origin[1]-y)<1e-7;}))));
  assert(floorIds.every(id=>id>=0),'Every expected native annular shoulder must exist');
  const result=await page.evaluate(async({dims,mode,side,floorIds})=>{
   const [r,h,d]=dims;
   if(mode==='preview'){yaw=-Math.PI/2;pitch=side==='top'?Math.PI/2:-Math.PI/2;draw();}
   else{cadGpuTest.setCamera({yaw:-Math.PI/2,pitch:side==='top'?Math.PI/2:-Math.PI/2});cadGpuTest.draw();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();}
   const screen=p=>{
    if(mode!=='preview')return cadGpuTest.project(p);
    const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,right),-dot(q,up)];},extent=[0,0];
    for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),q=project(p);
    return[width/2+q[0]*scale,height/2+20+q[1]*scale];
   };
   const pick=p=>{const q=screen(p);if(mode!=='preview'){const hit=cadGpuTest.pick(q[0],q[1]);return hit?hit.face:-1;}
    const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)];
    return id>=0?scene.faces[id].semantic_face_id:-1;
   };
   let rays=0;
   const check=(p,id)=>{const got=pick(p);if(got!==id)throw Error(`${mode}/${side} ray ${p}: expected native face ${id}, got ${got}`);rays++;};
   const centers=[[20,20],[60,20],[100,20],[20,60],[60,60],[100,60]];
   const outer=side==='top'?r:4.5;
   for(const [j,[x,y]] of centers.entries()){
    // The shoulder is an annulus: the central pilot remains empty.
    for(const dx of [-1,0,1])check([x+dx,y,d/2],-1);
    for(const sign of [-1,1]){
     check([x+sign*(3.2+outer)/2,y,d/2],floorIds[j]);
     check([x+sign*(outer+.7),y,d/2],side==='top'?1:0);
    }
   }
   check([60,40,d/2],side==='top'?1:0);
   if(mode==='preview'){const q=screen([60,40,d]);select(q[0],q[1]);}
   else{const q=screen([60,40,d]);cadGpuTest.pick(q[0],q[1]);}
   return{mode,side,dims,rays};
  },{dims,mode,side,floorIds});
  results.push(result);await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
 }
 await page.goto(`http://127.0.0.1:${server.address().port}/0/counterbored_fixture.stream.html`);
 await page.waitForFunction(()=>window.cadGpuTest);
 await page.evaluate(async()=>{await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();if(compactScene.instances.length!==32||Object.keys(compactScene.definitions).length!==5)throw Error('Nested definition sharing lost');const i=compactScene.instances.findIndex(i=>compactScene.instance_paths[i.id].join('/')==='top_plate');if(i<0)throw Error('Nested path missing');cadGpuTest.select(i,1);});
 await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);assert.match(await page.locator('#selection').textContent(),/top_plate/);
 await page.screenshot({path:'/tmp/cad-counterbore.png'});
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,instances:32,definitions:5,capturedSource:true,exactGeometryDownloads:0},null,2));
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));if(temporary)await fs.rm(temporary,{recursive:true,force:true});});
