// Independent angle/trapezoid oracle: source supplies constraints, not solved geometry.
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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-angular-sketch-'));
 const filename=path.join(root,'examples/constrained_angle_fixture.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[60,30,24],[50,15,18],[80,45,30]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['span','tilt_degrees','deck_top_width'][i]}=${dimensions[i]}`);
  const output=JSON.parse(execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{encoding:'utf8',stdio:'pipe'}));
  for(const [name,rank] of [['support_sketch',6],['deck_sketch',8]]){
   const report=output.sketch_reports[name];
   assert.equal(report.schema_version,2);assert.equal(report.status,'fully_constrained');
   assert.equal(report.rank,rank);assert.equal(report.degrees_of_freedom,0);
   assert(report.max_residual_mm<=report.tolerance_mm);
   assert(report.max_angular_residual_radians<=report.angular_tolerance_radians);
   assert.equal(report.solver_build_id,output.solver_build_id);
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
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/constrained_angle_fixture.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [span,degrees,top]=dimensions,theta=degrees*Math.PI/180;
   const cosine=Math.cos(theta),sine=Math.sin(theta),supportHeight=span*Math.tan(theta);
   const k=(36-top)/2,h=Math.sqrt(44*44-k*k),cy=h*(36+2*top)/(3*(36+top));
   const rotation=[[cosine,0,-sine],[0,1,0],[sine,0,cosine]];
   const mul=(m,v)=>m.map(row=>row.reduce((sum,value,j)=>sum+value*v[j],0));
   const seat=[10+span/2,40,6+supportHeight/2],shift=mul(rotation,[18,cy,0]);
   const origin=seat.map((value,j)=>value-shift[j]);
   const c=compactScene;
   if(c.instances.length!==9||Object.keys(c.definitions).length!==5)throw Error('Definition reuse lost');
   const index=c.instances.findIndex(i=>c.instance_paths[i.id].join('/')==='deck');
   if(index<0)throw Error('Deck missing');
   const deck=c.instances[index];
   const near=(a,b,label)=>{if(Math.abs(a-b)>1e-6)throw Error(label+': '+a+' != '+b)};
   const checkTransform=(instance,expectedOrigin,expectedRotation)=>{
    for(let j=0;j<3;j++)near(instance.transform.translation[j],expectedOrigin[j],'Wrong solved '+instance.id+' origin');
    // Compare transformed basis vectors; preview stores rotations as xyzw quaternions.
    const [x,y,z,w]=instance.transform.rotation;
    const actual=[[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],
                  [2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],
                  [2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]];
    for(let j=0;j<3;j++)for(let l=0;l<3;l++)near(actual[j][l],expectedRotation[j][l],'Wrong solved '+instance.id+' rotation');
   };
   checkTransform(deck,origin,rotation);
   const padOrigins={};
   for(const [name,centerPoint,normal] of [
    ['left_pad',[k/2,h/2,2],[-h/44,k/44,0]],
    ['right_pad',[36-k/2,h/2,2],[h/44,k/44,0]]]){
    const pad=c.instances.find(i=>c.instance_paths[i.id].join('/')===name);
    if(!pad)throw Error('Pad missing '+name);
    const local=[[0,normal[1],normal[0]],[0,-normal[0],normal[1]],[1,0,0]];
    const foot=mul(local,[1,4,0]);
    const translated=mul(rotation,centerPoint.map((value,j)=>value-foot[j]));
    const padOrigin=origin.map((value,j)=>value+translated[j]);
    const padRotation=rotation.map(row=>[0,1,2].map(j=>row.reduce((sum,value,l)=>sum+value*local[l][j],0)));
    checkTransform(pad,padOrigin,padRotation);padOrigins[name]=padOrigin;
   }
   const p=seat.map((v,i)=>v+4*rotation[i][2]);
   if(mode==='preview'){
    const rightAxis=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,rightAxis),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p);select(width/2+r[0]*scale,height/2+20+r[1]*scale);
    if(selectedInstance!==deck.id)throw Error('Canvas did not pick the solved deck '+JSON.stringify({selectedInstance,expected:deck.id,p,origin,seat}));
   }else{
    const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);
    if(hit?.instance!==index||hit.face<0)throw Error('GPU did not pick the solved deck '+JSON.stringify(hit));
   }
   return{mode,dimensions,deckOrigin:deck.transform.translation,padOrigins,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/deck/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-angular-fixture.png'});
  }
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSourceWithInitialGuesses:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
