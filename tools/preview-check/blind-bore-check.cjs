// Floors, unopened caps and through openings must remain distinct in every viewer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs/promises'),path=require('node:path'),http=require('node:http'),os=require('node:os');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
 const root=path.resolve(__dirname,'../..'),filename=path.join(root,'examples/blind_bore_pallet.star'),source=await fs.readFile(filename,'utf8');
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-blind-bore-'));
 const fixtures=[[8,12,28],[6,8,24],[10,16,32]];
 for(const [i,dims] of fixtures.entries()){
  const args=['build',filename,'--out-dir',path.join(temporary,String(i))];
  dims.forEach((n,j)=>args.push('--param',`${['socket_radius','socket_depth','plate_height'][j]}=${n}`));
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),args,{stdio:'pipe'});
 }
 server=http.createServer(async(req,res)=>{try{const name=new URL(req.url,'http://localhost').pathname;res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(await fs.readFile(path.join(temporary,name)));}catch{res.writeHead(404);res.end();}});
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],exact=[],results=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url());});
 for(const [i,dims] of fixtures.entries())for(const mode of ['preview','gpu','stream'])for(const side of ['top','bottom']){
  await page.goto(`http://127.0.0.1:${server.address().port}/${i}/pallet_inspection.${mode}.html`);
  if(mode!=='preview')await page.waitForFunction(()=>window.cadGpuTest);
  const result=await page.evaluate(async({dims,mode,side})=>{
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
   const top=[[20,20],[60,20],[100,20],[20,60],[60,60],[100,60]],bottom=[[40,40],[80,40]],through=[[8,8],[112,8],[8,72],[112,72]];
   // Independent design coordinates: inside each mouth sees the floor from its
   // opening side and the original unperforated cap from the opposite side.
   for(const [j,[x,y]] of top.entries())for(const dx of [-r/2,0,r/2])check([x+dx,y,d],side==='top'?7+2*j:0);
   for(const [j,[x,y]] of bottom.entries())for(const dx of [-2,0,2])check([x+dx,y,0],side==='bottom'?19+2*j:1);
   for(const [x,y] of through)for(const dx of [-1,0,1])check([x+dx,y,d/2],-1);
   check([60,40,d/2],side==='top'?1:0);
   if(mode==='preview'){const q=screen([60,40,d]);select(q[0],q[1]);}
   else{const q=screen([60,40,d]);cadGpuTest.pick(q[0],q[1]);}
   return{mode,side,dims,rays};
  },{dims,mode,side});
  results.push(result);await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);
  if(mode!=='preview')assert.deepEqual(await page.evaluate(()=>cadGpuStats.errors),[]);
 }
 await page.goto(`http://127.0.0.1:${server.address().port}/0/blind_bore_pallet.stream.html`);
 await page.waitForFunction(()=>window.cadGpuTest);
 await page.evaluate(async()=>{await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();if(compactScene.instances.length!==26||Object.keys(compactScene.definitions).length!==4)throw Error('Nested definition sharing lost');const i=compactScene.instances.findIndex(i=>compactScene.instance_paths[i.id].join('/')==='station_a/pallet');if(i<0)throw Error('Nested path missing');cadGpuTest.select(i,1);});
 await page.locator('#recipe-toggle').click();assert.equal(await page.locator('#recipe-source').textContent(),source);assert.match(await page.locator('#selection').textContent(),/station_a/);
 await page.screenshot({path:'/tmp/cad-blind-bore.png'});
 assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({results,instances:26,definitions:4,capturedSource:true,exactGeometryDownloads:0},null,2));
})().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));if(temporary)await fs.rm(temporary,{recursive:true,force:true});});
