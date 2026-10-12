// Independent two-disk membership and Boolean silhouette oracle.
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
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-circular-boolean-'));
 const filename=path.join(root,'examples/twin_lobe_fixture.star'),source=await fs.readFile(filename,'utf8');
 const fixtures=[[22,30,8],[20,26,6],[24,34,10]];
 for(const [index,dimensions] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(index))];
  for(let i=0;i<3;i++)args.push('--param',`${['left_radius','spacing','depth'][i]}=${dimensions[i]}`);
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
  await page.goto(`http://127.0.0.1:${server.address().port}/${index}/twin_lobe_fixture.${mode}.html`);
  if(mode==='preview')await page.locator('[data-view="top"]').click();
  else{await page.waitForFunction(()=>window.cadGpuTest);await page.evaluate(async()=>{cadGpuTest.setView('top');await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()})}
  const result=await page.evaluate(({dimensions,mode})=>{
   const [radius,spacing,depth]=dimensions;
   const c=compactScene;
   if(c.instances.length!==6||Object.keys(c.definitions).length!==5)throw Error('Definition reuse lost');
   const canvasProject=p=>{
    const rightAxis=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const project=q=>{q=sub(q,center);return[dot(q,rightAxis),-dot(q,up)]};
    const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]))}
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),q=project(p);
    return[width/2+q[0]*scale,height/2+20+q[1]*scale];
   };
   const pick=p=>{
    if(mode==='preview'){
     const q=canvasProject(p),id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)];return scene.faces.find(f=>f.id===id)?.instance_id||null;
    }
    const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);
    return hit?c.instances[hit.instance].id:null;
   };
   let rays=0;
   const counts={left_lobe:0,right_lobe:0,overlap:0,outside:0};
   for(let x=-radius-1.3;x<spacing+19;x+=3.7)for(let y=-radius-1.7;y<radius+2;y+=3.9){
    const ra=Math.hypot(x,y),rb=Math.hypot(x-spacing,y);
    if(Math.abs(ra-radius)<.35||Math.abs(rb-18)<.35||ra<4.5||rb<4.5)continue;
    const a=ra<radius,b=rb<18,expected=a?(b?'overlap':'left_lobe'):(b?'right_lobe':null);
    const actual=pick([x,y,depth+4]);
    if(actual!==expected)throw Error(`${mode} disk membership at ${x},${y}: expected ${expected}, got ${actual}`);
    counts[expected||'outside']++;rays++;
   }
   for(const count of Object.values(counts))if(count<8)throw Error('Insufficient region coverage '+JSON.stringify(counts));
   for(const [name,expected] of [['base',[0,0,0]],['left_lobe',[0,0,4]],['right_lobe',[0,0,4]],['overlap',[0,0,4]],['left_boss',[0,0,depth+4]],['right_boss',[spacing,0,depth+4]]]){
    const item=c.instances.find(i=>i.id===name);if(!item)throw Error('Missing occurrence '+name);
    for(let j=0;j<3;j++)if(Math.abs(item.transform.translation[j]-expected[j])>1e-7)throw Error('Wrong placement '+name);
   }
   const point=[-radius/2,0,depth+4];
   if(pick(point)!=='left_lobe')throw Error('Left lobe selection failed');
   if(mode==='preview'){const q=canvasProject(point);select(q[0],q[1]);}
   return{mode,dimensions,rays,counts,instances:c.instances.length,definitions:Object.keys(c.definitions).length};
  },{dimensions,mode});
  results.push(result);
  await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  assert.match(await page.locator('#selection').textContent(),/left_lobe/);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
  if(index===0&&mode==='stream'){
   await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish()});
   await page.screenshot({path:'/tmp/cad-circular-boolean.png'});
  }
 }
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,capturedRecipeSource:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
