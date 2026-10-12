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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-drill-sketch-'));
 const filename=path.join(root,'examples/constrained_drill_jig.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[25,4,8],[20,3,6],[30,5,10]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['pitch','bore_radius','thickness'][i]}=${dimensions[i]}`);
  const output=JSON.parse(execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{encoding:'utf8',stdio:'pipe'}));
  for(const [name,rank] of [['guide_layout',20]]){
   const report=output.sketch_reports[name];
   assert.equal(report.schema_version,2);assert.equal(report.status,'fully_constrained');
   assert.equal(report.rank,rank);assert.equal(report.degrees_of_freedom,0);
   assert(report.max_residual_mm<=report.tolerance_mm);
   assert(report.max_angular_residual_radians<=report.angular_tolerance_radians);
   assert.equal(report.solver_build_id,output.solver_build_id);
  }
  const solved=output.sketch_reports.guide_layout.points;
  for(const [name,x] of [['left',50-dimensions[0]],['middle',50],['right',50+dimensions[0]]]){
   assert(Math.abs(solved[name][0]-x)<1e-6);assert(Math.abs(solved[name][1]-30)<1e-6);
   assert(Math.abs(Math.hypot(...solved[name+'_rim'].map((v,j)=>v-solved[name][j]))-dimensions[1])<1e-6);
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
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/constrained_drill_jig.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [spacing,radius,thickness]=dimensions;
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
   for(const [i,x] of [50-spacing,50,50+spacing].entries()){
    const guide=c.instances.find(instance=>instance.id==='guide_'+i);
    if(!guide)throw Error('Missing guide');
    for(let j=0;j<3;j++)if(Math.abs(guide.transform.translation[j]-[x,30,12][j])>1e-7)throw Error('Wrong guide placement');
    for(const [delta,expected] of [[0,null],[(radius-1)/2,null],[radius-.5,'guide_'+i],[radius+.5,'plate']]){
     for(const axis of [0,1]){
      const point=[x,30,12+thickness];point[axis]+=delta;
      const actual=pick(point);if(actual!==expected)throw Error(`${mode} bore ${i} ray ${point}: expected ${expected}, got ${actual}`);
      rays++;
     }
    }
   }
   if(pick([10,30,12+thickness])!=='plate')throw Error('Plate selection failed');
   return{mode,dimensions,rays,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/plate/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-drill-fixture.png'});
  }
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSourceWithInitialGuesses:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
