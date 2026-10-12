// Independent solved bore silhouette oracle: source supplies constraints, not solved geometry.
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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-capsule-sketch-'));
 const filename=path.join(root,'examples/constrained_capsule_link.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[60,10,4],[40,8,5],[80,14,3]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['span','outer_radius','bore_radius'][i]}=${dimensions[i]}`);
  const output=JSON.parse(execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{encoding:'utf8',stdio:'pipe'}));
  for(const [name,rank] of [['capsule_layout',16]]){
   const report=output.sketch_reports[name];
   assert.equal(report.schema_version,2);assert.equal(report.status,'fully_constrained');
   assert.equal(report.rank,rank);assert.equal(report.degrees_of_freedom,0);
   assert(report.max_residual_mm<=report.tolerance_mm);
   assert(report.max_angular_residual_radians<=report.angular_tolerance_radians);
   assert.equal(report.solver_build_id,output.solver_build_id);
  }
  const solved=output.sketch_reports.capsule_layout.points;
  for(const [name,point] of [['a',[0,-dimensions[1]]],['b',[dimensions[0],-dimensions[1]]],['c',[dimensions[0],dimensions[1]]],['d',[0,dimensions[1]]]]){
   for(let j=0;j<2;j++)assert(Math.abs(solved[name][j]-point[j])<1e-6);
  }
 }
 server=http.createServer(async(req,res)=>{
  try{const name=new URL(req.url,'http://localhost').pathname;res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(temporary,name)))}
  catch{res.writeHead(404);res.end()}
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],exact=[],results=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
 for(const [index,dimensions] of fixtures.entries())for(const mode of ['preview','gpu','stream']){
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/constrained_capsule_link.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [span,radius,bore]=dimensions;
   const c=compactScene;
   if(c.instances.length!==6||Object.keys(c.definitions).length!==3)throw Error('Definition reuse lost');
   const canvasProject=p=>{
    const rightAxis=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,rightAxis),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),q=project(p);
    return[width/2+q[0]*scale,height/2+20+q[1]*scale];
   };
   const pick=p=>{
    if(mode==='preview'){
     const q=canvasProject(p);select(q[0],q[1]);return selectedInstance;
    }
    const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);
    return hit?c.instances[hit.instance].id:null;
   };
   let rays=0;
   const samples=[
    [[span/2,0,32],'upper_link'], [[span/2,radius-.5,32],'upper_link'],
    [[-radius+.5,0,32],'upper_link'], [[span+radius-.5,0,32],'upper_link'],
    [[-radius-.5,0,32],null], [[span+radius+.5,0,32],null],
    [[span/2,radius+.5,32],null], [[span/2,-radius-.5,32],null],
    [[0,0,34],'pin_0'], [[span,0,34],'pin_1'],
    [[0,bore-.25,32],'pin_0'], [[span,bore-.25,32],'pin_1'],
    [[0,bore+.5,32],'upper_link'], [[span,bore+.5,32],'upper_link'],
   ];
   for(const [point,expected] of samples){
    const actual=pick(point);if(actual!==expected)throw Error(`${mode} ray ${point}: expected ${expected}, got ${actual}`);rays++;
   }
   for(const [name,expected] of [['lower_link',[0,0,6]],['upper_link',[0,0,24]],['spacer_0',[0,0,14]],['spacer_1',[span,0,14]],['pin_0',[0,0,0]],['pin_1',[span,0,0]]]){
    const part=c.instances.find(instance=>instance.id===name);if(!part)throw Error('Missing part '+name);
    for(let j=0;j<3;j++)if(Math.abs(part.transform.translation[j]-expected[j])>1e-7)throw Error('Wrong placement '+name);
   }
   if(pick([span/2,0,32])!=='upper_link')throw Error('Link selection failed');
   return{mode,dimensions,rays,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/upper_link/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-capsule-fixture.png'});
  }
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSourceWithInitialGuesses:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
