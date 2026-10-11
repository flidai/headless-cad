// Selective-load evidence for the authored twelve-station workload.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const os=require('node:os');
const {execFileSync}=require('node:child_process');
(async()=>{
  const origin=process.env.CAD_STREAM_URL || 'http://localhost:8080/workcell_line/workcell_line.stream.html';
  const camera={yaw:-Math.PI/2,pitch:Math.PI/2,target:[50,30,21],halfHeight:50};
  const url=new URL(origin);url.hash='camera='+encodeURIComponent(JSON.stringify(camera));
  const software=process.env.CAD_GPU_HARDWARE!=='1';
  const args=['--no-sandbox'];if(software)args.push('--use-angle=swiftshader','--enable-unsafe-swiftshader');
  const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args});
  try{
    const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
    const requests=[],errors=[];
    page.on('request',r=>requests.push(r.url()));page.on('pageerror',e=>errors.push(e.message));
    const start=performance.now();await page.goto(url.href);
    await page.waitForFunction(()=>window.cadGpuStats?.ready);
    const proxyReadyMs=performance.now()-start;
    await page.evaluate(()=>window.cadGpuTest.whenIdle());
    await page.evaluate(()=>{window.cadGpuTest.draw();window.cadGpuTest.finish();});
    const detailReadyMs=performance.now()-start;
    const snapshot=()=>page.evaluate(()=>{
      const gl=document.querySelector('canvas').getContext('webgl2'),ext=gl.getExtension('WEBGL_debug_renderer_info');
      return {stats:{...window.cadGpuStats},renderer:ext?gl.getParameter(ext.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER),camera:window.cadGpuTest.snapshot(),meshAssets:Object.keys(window.compactScene.mesh_assets).length,assembly_hash:window.compactScene.content_hash,recipe_context_hash:window.recipeContext.recipe_context_hash};
    });
    const first=await snapshot();const firstRequests=requests.slice();
    const meshes=list=>list.filter(value=>/\/mesh-[a-f0-9]{64}\.display\.json$/.test(value));
    assert.equal(meshes(firstRequests).length,4,'initial camera should request only three shared frames and one tool');
    assert.equal(first.meshAssets,15);
    assert.equal(first.stats.visibleCount,5);assert.equal(first.stats.proxyCount,0);
    assert.equal(first.stats.residentMeshCount,4);assert.equal(first.stats.loadingCount,0);
    const panStart=performance.now();
    await page.evaluate(()=>{window.cadGpuTest.setCamera({target:[650,330,21]});window.cadGpuTest.draw();});
    await page.evaluate(()=>window.cadGpuTest.whenIdle());
    await page.evaluate(()=>{window.cadGpuTest.draw();window.cadGpuTest.finish();});
    const second=await snapshot(),panReadyMs=performance.now()-panStart;
    assert.equal(meshes(requests).length,5,'second station should reuse three frames and fetch one tool');
    assert.equal(second.stats.visibleCount,5);assert.equal(second.stats.proxyCount,0);
    assert.equal(second.stats.residentMeshCount,5);assert.equal(second.stats.loadingCount,0);
    assert.equal(requests.filter(value=>/\.brep\.json(?:$|\?)/.test(value)).length,0);
    assert.deepEqual(errors,[]);
    console.log(JSON.stringify({schema_version:1,revision:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),date_utc:new Date().toISOString(),url:url.href,browser:browser.version(),software_renderer_requested:software,environment:{os:os.type(),release:os.release(),arch:os.arch(),cpu:os.cpus()[0]?.model,logical_cpus:os.cpus().length},viewport:[1440,1000],device_scale_factor:1,proxy_ready_ms:proxyReadyMs,first_detail_ready_ms:detailReadyMs,pan_detail_ready_ms:panReadyMs,first_requests:firstRequests,all_requests:requests,exact_geometry_requests:0,first,second,errors},null,2));
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
