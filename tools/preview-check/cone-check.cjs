// Independent ray/cone, ray/cylinder and ray/annulus oracle for a chamfered,
// tapered body with a countersink and blind bore. Display is tested away
// from silhouette/edge ambiguity; native faces remain the inspection identity.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const http = require('node:http');
const os = require('node:os');
const { execFileSync } = require('node:child_process');
let temporary, server, browser;
(async () => {
  const root = path.resolve(__dirname, '../..');
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-cone-check-'));
  const profile = [[0,0],[10,0],[12,2],[12,8],[8,18],[6,18],[3,12],[3,4],[0,4]];
  const source = `# Chamfered and tapered countersunk body, authored as a meridian in [radius,z].\np=polygon(id='meridian',points=${JSON.stringify(profile)})\nb=revolve(id='body',profile=p)\nd=part(id='body',body=b)\na=assembly(id='body',instances=[instance(id='body',part=d)])\nassembly_output(name='body',assembly=a)\n`;
  const recipe = path.join(temporary,'body.star'), output = path.join(temporary,'out');
  await fs.writeFile(recipe, source);
  execFileSync(process.env.CAD_BIN || path.join(root,'target/debug/cad'), ['build',recipe,'--out-dir',output], {stdio:'pipe'});
  const errors=[], exact=[], results=[];
  server = http.createServer(async (req,res) => {
    try {
      const name=path.basename(new URL(req.url,'http://localhost').pathname);
      if(name==='favicon.ico'){res.writeHead(204);res.end();return;}
      res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');
      res.end(await fs.readFile(path.join(output,name)));
    } catch {res.writeHead(404);res.end();}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const page=await browser.newPage({viewport:{width:1200,height:900}});
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url());});
  for(const mode of ['preview','gpu','stream']) {
    await page.goto(`http://127.0.0.1:${server.address().port}/body.${mode}.html`);
    if(mode!=='preview')await page.waitForFunction(()=>window.cadGpuTest);
    const coverage={};
    for(const view of ['top','iso']) {
      if(mode!=='preview')await page.evaluate(async view=>{cadGpuTest.setView(view);await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();},view);
      else await page.locator(`[data-view="${view}"]`).click();
      const result=await page.evaluate(({mode,view})=>{
        const a=mode==='preview'?yaw:-Math.PI/3,b=mode==='preview'?pitch:Math.PI/5;
        const eye=view==='top'?[0,0,1]:[Math.cos(a)*Math.cos(b),Math.sin(a)*Math.cos(b),Math.sin(b)];
        // Face numbers follow the authored non-axis meridian edges. Values below
        // are explicit engineering dimensions, not recovered from render meshes.
        // [lower radius, lower z, upper radius, upper z, native face].
        const walls=[[10,0,12,2,1],[12,2,12,8,2],[12,8,8,18,3],[3,12,6,18,5],[3,4,3,12,6]];
        const planes=[[0,0,10,0],[18,6,8,4],[4,0,3,7]];
        function oracle(p) {
          const hits=[];let uncertain=false;
          for(const [r0,z0,r1,z1,face] of walls) {
            const slope=(r1-r0)/(z1-z0),r=r0+slope*(p[2]-z0),d=slope*eye[2];
            // Substitute p+t*eye into x²+y²=(r0+slope*(z-z0))².
            const A=eye[0]**2+eye[1]**2-d*d;
            const B=2*(p[0]*eye[0]+p[1]*eye[1]-r*d);
            const C=p[0]**2+p[1]**2-r*r,D=B*B-4*A*C;
            let roots=[];
            if(Math.abs(A)<1e-12) {
              if(Math.abs(B)>1e-12)roots=[-C/B];
            } else {
              if(Math.abs(D)<3)uncertain=true;
              if(D>0)roots=[(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)];
            }
            for(const t of roots) {
              const z=p[2]+t*eye[2];
              if(Math.min(Math.abs(z-z0),Math.abs(z-z1))<.3)uncertain=true;
              if(z>z0&&z<z1)hits.push({t,face});
            }
          }
          for(const [z,inner,outer,face] of planes) {
            const t=(z-p[2])/eye[2],r=Math.hypot(p[0]+t*eye[0],p[1]+t*eye[1]);
            if(Math.abs(r-outer)<.3||(inner>0&&Math.abs(r-inner)<.3))uncertain=true;
            if(r>=inner&&r<outer)hits.push({t,face});
          }
          return {uncertain,hit:hits.sort((a,b)=>b.t-a.t)[0]};
        }
        let canvasProject;
        if(mode==='preview') {
          const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
          const project=p=>{const d=sub(p,center);return [dot(d,right),-dot(d,up)];};
          const extent=[0,0];
          for(const f of scene.faces)for(const p of f.points){const q=project(p);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));}
          const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1));
          canvasProject=p=>{const q=project(p);return [width/2+q[0]*scale,height/2+20+q[1]*scale];};
        }
        function pick(p) {
          if(mode!=='preview'){const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?hit.face:null;}
          const q=canvasProject(p);
          if(q[0]<0||q[0]>=width||q[1]<0||q[1]>=height)return null;
          const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)];
          const f=scene.faces.find(f=>f.id===id);return f?f.local_face_id:null;
        }
        let samples=0,misses=0;const faceSamples={};
        for(let x=-15.23;x<16;x+=2.2)for(let y=-15.37;y<16;y+=2.2) {
          const p=[x,y,9],expected=oracle(p);if(expected.uncertain)continue;
          const actual=pick(p),wanted=expected.hit?.face??null;
          if(actual!==wanted)throw Error(`${mode}/${view} ray ${JSON.stringify(p)} expected face ${wanted}, got ${actual}`);
          if(actual===null)misses++;else faceSamples[actual]=(faceSamples[actual]||0)+1;
          samples++;
        }
        if(view==='iso') {
          // The shallow visible band of the lower chamfer can fall between the
          // modest grid rays. Aim at its mid-height on the camera-facing side.
          const p=[11*Math.cos(a),11*Math.sin(a),1],expected=oracle(p);
          if(expected.hit?.face!==1)throw Error('Authored chamfer witness must be visible');
          if(pick(p)!==1)throw Error(`${mode}: camera-facing chamfer has wrong native face`);
          faceSamples[1]=(faceSamples[1]||0)+1;samples++;
        }
        if(samples<65||misses<15)throw Error('Insufficient independent ray coverage');
        return {mode,view,samples,misses,faceSamples};
      },{mode,view});
      results.push(result);
      for(const [face,count] of Object.entries(result.faceSamples))coverage[face]=(coverage[face]||0)+count;
    }
    for(const face of [1,2,3,4,5,7])assert.ok(coverage[face]>0, `${mode}: missing chamfer, taper, cap, countersink or bore floor face ${face}`);
    await page.locator('#recipe-toggle').click();
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    if(mode==='stream')await page.screenshot({path:'/tmp/cad-conical-countersink.png'});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({results,capturedRecipe:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
  if(browser)await browser.close();
  if(server)await new Promise(resolve=>server.close(resolve));
  if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
