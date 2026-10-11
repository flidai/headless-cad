// Cross-level datum attachment display acceptance. The expected world points
// follow authored integer rotations and dimensions, independently of the viewer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const http=require('node:http');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
  const root=path.resolve(__dirname,'../..');
  temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-attachment-check-'));
  const recipes=[];
  for(const name of ['instrumented_cartridge','mounted_drive_module']){
    const recipe=path.join(root,`examples/${name}.star`),source=await fs.readFile(recipe,'utf8');
    assert.match(source,/target_path\s*=\s*\["cartridge","housing"\]/);
    if(name==='mounted_drive_module')assert.match(source,/source_path\s*=\s*\["cartridge","housing"\]/);
    recipes.push({name,source});
    for(const spacing of [80,120])execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',recipe,'--param',`module_spacing=${spacing}`,'--out-dir',path.join(temporary,name,String(spacing))],{stdio:'pipe'});
  }
  const errors=[],exact=[],results=[];
  server=http.createServer(async(req,res)=>{
    try{
      const pathname=new URL(req.url,'http://localhost').pathname;
      if(pathname==='/favicon.ico'){res.writeHead(204);res.end();return;}
      res.setHeader('Content-Type',pathname.endsWith('.html')?'text/html':'application/json');
      res.end(await fs.readFile(path.join(temporary,pathname)));
    }catch{res.writeHead(404);res.end();}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(request.url()))exact.push(request.url());});
  for(const {name,source} of recipes)for(const spacing of [80,120])for(const mode of ['preview','gpu','stream']){
    await page.goto(`http://127.0.0.1:${server.address().port}/${name}/${spacing}/${name}.${mode}.html`);
    if(mode==='preview')await page.locator('[data-view="top"]').click();
    else{
      await page.waitForFunction(()=>window.cadGpuTest);
      await page.evaluate(async spacing=>{cadGpuTest.setView('top');cadGpuTest.setCamera({target:[spacing,22,20],halfHeight:28});cadGpuTest.render();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();},spacing);
    }
    const result=await page.evaluate(({mode,spacing})=>{
      const c=compactScene;
      if(c.instances.length!==11||Object.keys(c.definitions).length!==7)throw Error('Occurrence/definition sharing lost');
      function occurrence(name){
        const index=c.instances.findIndex(i=>JSON.stringify(c.instance_paths[i.id])===JSON.stringify([name]));
        if(index<0)throw Error(`Missing root occurrence ${name}`);
        return {index,...c.instances[index]};
      }
      const mount=occurrence('sensor_mount'),sensor=occurrence('sensor');
      for(const [instance,expected] of [[mount,[spacing+5,18,26]],[sensor,[spacing,22,20]]]){
        if(instance.transform.translation.some((v,a)=>Math.abs(v-expected[a])>1e-9))throw Error(`Wrong attached world origin ${instance.id}`);
        // Rz90*Ry90 maps local [x,y,z] to world [-y,z,-x].
        const [x,y,z,w]=instance.transform.rotation;
        const matrix=[[1-2*(y*y+z*z),2*(x*y-z*w),2*(x*z+y*w)],[2*(x*y+z*w),1-2*(x*x+z*z),2*(y*z-x*w)],[2*(x*z-y*w),2*(y*z+x*w),1-2*(x*x+y*y)]];
        const expectedMatrix=[[0,-1,0],[0,0,1],[-1,0,0]];
        if(matrix.some((row,a)=>row.some((v,b)=>Math.abs(v-expectedMatrix[a][b])>1e-9)))throw Error(`Wrong attached world orientation ${instance.id}`);
      }
      function projectCanvas(p){
        const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
        const projected=p=>{const d=sub(p,center);return[dot(d,right),-dot(d,up)];};
        const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=projected(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));}
        const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),q=projected(p);
        return[width/2+q[0]*scale,height/2+20+q[1]*scale];
      }
      const picks=[];
      // Interior points on the expected top-facing surfaces, clear of adjacent
      // housing geometry. The sensor is outside the targeted nested module.
      for(const [instance,point] of [[mount,[spacing+4.5,21.75,26]],[sensor,[spacing,30,23]]]){
        if(mode==='preview'){
          const q=projectCanvas(point);select(q[0],q[1]);
          if(selectedInstance!==instance.id)throw Error(`Canvas picked ${selectedInstance} instead of ${instance.id}`);
        }else{
          const q=cadGpuTest.project(point),hit=cadGpuTest.pick(q[0],q[1]);
          if(hit?.instance!==instance.index||hit.face<0)throw Error(`GPU picked ${JSON.stringify(hit)} instead of ${instance.id}`);
        }
        picks.push(instance.id);
      }
      return{mode,spacing,instances:c.instances.length,definitions:Object.keys(c.definitions).length,picks,parameter:recipeContext.parameters.module_spacing};
    },{mode,spacing});
    assert.equal(result.parameter,spacing);results.push({recipe:name,...result});
    await page.locator('#recipe-toggle').click();
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    assert.match(await page.locator('#selection').textContent(),/sensor/);
    if(mode==='stream'&&spacing===120){
      await page.evaluate(async()=>{cadGpuTest.setView('iso');cadGpuTest.fit();cadGpuTest.render();await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();});
      if(name==='mounted_drive_module'){
        await page.locator('#recipe-panel').scrollIntoViewIfNeeded();
        await page.locator('#recipe-source').evaluate(element=>{element.scrollTop=element.scrollHeight;});
      }
      await page.screenshot({path:name==='mounted_drive_module'?'/tmp/cad-mounted-drive-module.png':'/tmp/cad-instrumented-attachments.png'});
    }
  }
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({results,capturedRecipe:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
  if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
