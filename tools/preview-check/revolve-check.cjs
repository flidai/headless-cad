// Independent exact ray/cylinder and ray/annulus oracle for a connected stepped
// solid with a counterbore and blind bore. Display approximation is tested away
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
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-revolve-check-'));
  const profile = [[0,0],[12,0],[12,8],[9,8],[9,18],[6,18],[6,12],[3,12],[3,4],[0,4]];
  const source = `# Connected counterbored body, authored as a meridian in [radius,z].\np=polygon(id='meridian',points=${JSON.stringify(profile)})\nb=revolve(id='body',profile=p)\nd=part(id='body',body=b)\na=assembly(id='body',instances=[instance(id='body',part=d)])\nassembly_output(name='body',assembly=a)\n`;
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
        const cylinders=[[12,0,8,1],[9,8,18,3],[6,12,18,5],[3,4,12,7]];
        const planes=[[0,0,12,0],[8,9,12,2],[18,6,9,4],[12,3,6,6],[4,0,3,8]];
        function oracle(p) {
          const hits=[];let uncertain=false;
          const A=eye[0]**2+eye[1]**2;
          if(A>1e-15)for(const [r,z0,z1,face] of cylinders) {
            const B=2*(p[0]*eye[0]+p[1]*eye[1]),C=p[0]**2+p[1]**2-r*r,D=B*B-4*A*C;
            if(Math.abs(D)<3)uncertain=true;
            if(D>0)for(const t of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)]) {
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
        for(let x=-15.23;x<16;x+=1.65)for(let y=-15.37;y<16;y+=1.65) {
          const p=[x,y,9],expected=oracle(p);if(expected.uncertain)continue;
          const actual=pick(p),wanted=expected.hit?.face??null;
          if(actual!==wanted)throw Error(`${mode}/${view} ray ${JSON.stringify(p)} expected face ${wanted}, got ${actual}`);
          if(actual===null)misses++;else faceSamples[actual]=(faceSamples[actual]||0)+1;
          samples++;
        }
        if(samples<100||misses<20)throw Error('Insufficient independent ray coverage');
        return {mode,view,samples,misses,faceSamples};
      },{mode,view});
      results.push(result);
      for(const [face,count] of Object.entries(result.faceSamples))coverage[face]=(coverage[face]||0)+count;
    }
    for(const face of [1,2,3,4,5,6,8])assert.ok(coverage[face]>0, `${mode}: missing outer wall, shoulder, bore wall or bore floor face ${face}`);
    await page.locator('#recipe-toggle').click();
    assert.equal(await page.locator('#recipe-source').textContent(),source);
    if(mode==='stream')await page.screenshot({path:'/tmp/cad-revolved-counterbore.png'});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({results,capturedRecipe:true,exactGeometryDownloads:0},null,2));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{
  if(browser)await browser.close();
  if(server)await new Promise(resolve=>server.close(resolve));
  if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
