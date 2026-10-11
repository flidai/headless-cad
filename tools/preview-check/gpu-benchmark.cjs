// A reproducible local rendering workload, not a discrete-GPU performance claim.
const {chromium} = require('playwright');
const os = require('node:os');
const {execFileSync} = require('node:child_process');
const assert = require('node:assert/strict');
(async () => {
  const url = process.env.CAD_GPU_URL || 'http://localhost:8080/display_benchmark/display_rack.gpu.html';
  const software = process.env.CAD_GPU_HARDWARE !== '1';
  const args = ['--no-sandbox'];
  if (software) args.push('--use-angle=swiftshader', '--enable-unsafe-swiftshader');
  const browser = await chromium.launch({executablePath:process.env.CAD_BROWSER || undefined,args});
  try {
    const page = await browser.newPage({viewport:{width:1920,height:1080},deviceScaleFactor:1});
    const requests=[],errors=[];
    page.on('request',request=>requests.push(request.url()));
    page.on('pageerror',error=>errors.push(error.message));
    const start=performance.now();
    const response=await page.goto(url);
    assert.ok(response?.ok(),'benchmark artifact must load');
    await page.waitForFunction(()=>window.cadGpuStats?.ready === true,{},{timeout:30000});
    await page.evaluate(()=>window.cadGpuTest.finish());
    const firstViewMs=performance.now()-start;
    const result=await page.evaluate(async()=>{
      const hook=window.cadGpuTest;
      if(!hook?.draw || !hook?.setCamera)throw Error('Missing GPU benchmark hooks');
      const canvas=document.querySelector('canvas'),gl=canvas.getContext('webgl2');
      const info=gl.getExtension('WEBGL_debug_renderer_info');
      const renderer=info?gl.getParameter(info.UNMASKED_RENDERER_WEBGL):gl.getParameter(gl.RENDERER);
      const vendor=info?gl.getParameter(info.UNMASKED_VENDOR_WEBGL):gl.getParameter(gl.VENDOR);
      const samples=[];
      // Fixed camera trace; timings include GPU completion, not only submission.
      for(let i=0;i<30;i++){
        const begin=performance.now();
        hook.setCamera({yaw:-.75+i*.025,pitch:.6,zoom:1});
        hook.draw();
        gl.finish();
        samples.push(performance.now()-begin);
        await new Promise(requestAnimationFrame);
      }
      if(gl.getError()!==gl.NO_ERROR)throw Error('WebGL error during benchmark');
      return {renderer,vendor,canvas_pixels:[canvas.width,canvas.height],assembly_hash:window.compactScene.content_hash,recipe_context_hash:window.recipeContext?.recipe_context_hash,parameters:window.recipeContext?.parameters,stats:{...window.cadGpuStats},frame_wall_ms:samples};
    });
    assert.deepEqual(errors,[]);
    assert.equal(requests.filter(url=>/\.brep\.json(?:$|\?)/.test(url)).length,0);
    const sorted=[...result.frame_wall_ms].sort((a,b)=>a-b);
    console.log(JSON.stringify({
      schema_version:1,revision:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8'}).trim(),
      date_utc:new Date().toISOString(),url,browser:browser.version(),software_renderer_requested:software,
      environment:{os:os.type(),release:os.release(),arch:os.arch(),cpu:os.cpus()[0]?.model,logical_cpus:os.cpus().length},
      viewport:[1920,1080],device_scale_factor:1,first_view_ms:firstViewMs,
      timing_scope:'fresh browser page navigation to ready; 30 fixed-camera synchronous draw+finish wall times, not presentation latency',
      requests,exact_geometry_requests:0,errors,...result,
      frame_wall_median_ms:sorted[Math.floor(sorted.length/2)],frame_wall_p95_ms:sorted[Math.ceil(sorted.length*.95)-1]
    },null,2));
  } finally {await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
