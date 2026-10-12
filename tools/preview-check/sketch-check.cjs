// Three-side triangle oracle: the recipe only supplies guesses and dimensions.
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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-sketch-'));
 const filename=path.join(root,'examples/constrained_slope_fixture.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[80,60,70],[70,55,85],[100,80,60]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['span','left_length','right_length'][i]}=${dimensions[i]}`);
  const output=JSON.parse(execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{encoding:'utf8',stdio:'pipe'}));
  const report=output.sketch_reports.support_sketch;
  assert.equal(report.status,'fully_constrained');assert.equal(report.degrees_of_freedom,0);
  assert(report.max_residual_mm<=report.tolerance_mm);assert.equal(report.solver_build_id,output.solver_build_id);
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
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/constrained_slope_fixture.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [span,left,right]=dimensions,x=(left*left+span*span-right*right)/(2*span),y=Math.sqrt(left*left-x*x),n=[y/right,0,(span-x)/right],axis=[n[2],0,-n[0]],seat=[10+(span+x)/2,30,6+y/2];
   const c=compactScene;
   if(c.instances.length!==9||Object.keys(c.definitions).length!==5)throw Error('Definition reuse lost');
   const index=c.instances.findIndex(i=>c.instance_paths[i.id].join('/')==='deck');
   if(index<0)throw Error('Deck missing');
   const deck=c.instances[index];
   for(let i=0;i<3;i++)if(Math.abs(deck.transform.translation[i]-(seat[i]-12*axis[i]-(i===1?24:0)))>1e-6)throw Error('Wrong solved deck placement');
   const p=seat.map((v,i)=>v+3*n[i]);
   if(mode==='preview'){
    const rightAxis=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,rightAxis),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p);select(width/2+r[0]*scale,height/2+20+r[1]*scale);
    if(selectedInstance!==deck.id)throw Error('Canvas did not pick the solved deck');
   }else{
    const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);
    if(hit?.instance!==index||hit.face<0)throw Error('GPU did not pick the solved deck '+JSON.stringify(hit));
   }
   return{mode,dimensions,deckOrigin:deck.transform.translation,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/deck/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-constrained-fixture.png'});
  }
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSourceWithInitialGuesses:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
