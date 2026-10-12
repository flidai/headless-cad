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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-concave-profile-'));
 const filename=path.join(root,'examples/split_bearing_block.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[14,16,2],[10,12,0],[18,24,3]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['seat_radius','depth','jaw_gap'][i]}=${dimensions[i]}`);
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{encoding:'utf8',stdio:'pipe'});
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
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/split_bearing_block.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [radius,depth,gap]=dimensions;
   const c=compactScene;
   if(c.instances.length!==7||Object.keys(c.definitions).length!==3)throw Error('Definition reuse lost');
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
   for(const [i,xy] of [[10,12],[70,12],[10,68+gap],[70,68+gap]].entries()){
    const sleeve=c.instances.find(instance=>instance.id==='sleeve_'+i);
    if(!sleeve)throw Error('Sleeve missing');
    const expected=[...xy,-2];
    for(let j=0;j<3;j++)if(Math.abs(sleeve.transform.translation[j]-expected[j])>1e-7)throw Error('Wrong sleeve placement');
    for(const [delta,part] of [[0,null],[1,null],[2.5,sleeve.id],[3.5,i<2?'lower_jaw':'upper_jaw']]){
     const point=[xy[0]+delta,xy[1],depth+2],actual=pick(point);
     if(actual!==part)throw Error(`${mode} ray ${point}: expected ${part}, got ${actual}`);rays++;
    }
   }
   if(pick([5,25,depth])!=='lower_jaw')throw Error('Jaw selection failed');
   return{mode,dimensions,rays,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/lower_jaw/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-concave-profile.png'});
  }
 }
 // The isolated native jaw exposes the inward seat that the assembly shaft hides.
 for(const [index,dimensions] of fixtures.entries()){
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/jaw.preview.html`);
  await page.locator('[data-view="top"]').click();
  const result=await page.evaluate(dimensions=>{
   const [radius,depth]=dimensions;
   const rightAxis=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
   const project=q=>{q=sub(q,center);return[dot(q,rightAxis),-dot(q,up)]};
   const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
   const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1));
   const screen=(x,y)=>{const q=project([x,y,depth]);return[width/2+q[0]*scale,height/2+20+q[1]*scale]};
   const hit=(x,y)=>{const q=screen(x,y);return faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)]>=0};
   let rays=0;
   for(const [x,y,expected] of [[40,40-radius/2,false],[40,40-radius-.5,true],[40-radius-.5,39.5,true],[40+radius+.5,39.5,true],[40,41,false],[10,12,false],[70,12,false],[5,25,true]]){
    if(hit(x,y)!==expected)throw Error('Incorrect isolated concave silhouette '+[x,y]);rays++;
   }
   // Independent circular-section rays across the entire recess, away from edges.
   for(let x=40-radius+.5;x<40+radius;x+=1.5)for(let y=40-radius-.5;y<39.5;y+=1.5){
    const distance=Math.hypot(x-40,y-40);if(Math.abs(distance-radius)<.3)continue;
    if(hit(x,y)!==(distance>radius))throw Error('Incorrect circular recess '+[x,y]);rays++;
   }
   const picked=screen(5,25);select(picked[0],picked[1]);return{mode:'isolated-jaw-canvas',dimensions,rays};
  },dimensions);
  results.push(result);await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
 }

 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedSourceWithInitialGuesses:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
