// Independent ray/surface oracle for a nonconvex prism with polygon and circular holes.
// CAD_DRILLED_DENSE=1 checks a grid with four times as many candidate rays.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const path = require('node:path');
const http = require('node:http');
const os = require('node:os');
const { execFileSync } = require('node:child_process');
(async () => {
  const root = path.resolve(__dirname, '../..');
  const temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-drilled-'));
  const outer = [[0,0],[80,0],[80,70],[50,70],[50,35],[30,35],[30,70],[0,70]];
  const holes = [[[8,8],[8,16],[18,16],[18,8]]];
  const circles = [{center:[24,24],radius:5},{center:[63,20],radius:8},{center:[15,48],radius:6}];
  const height = 8;
  const source = `# Exact circular holes in a nonconvex, polygon-holed plate.\np = polygon(id='profile', points=${JSON.stringify(outer)}, holes=${JSON.stringify(holes)}, circular_holes=${JSON.stringify(circles)})\nb = extrude(id='plate', profile=p, height=${height})\nd = part(id='plate', body=b)\na = assembly(id='plate', instances=[instance(id='plate', part=d)])\nassembly_output(name='plate', assembly=a)\n`;
  const sourcePath = path.join(temporary, 'plate.star'), output = path.join(temporary, 'out');
  await fs.writeFile(sourcePath, source);
  execFileSync(process.env.CAD_BIN || path.join(root, 'target/debug/cad'), ['build',sourcePath,'--out-dir',output], {stdio:'pipe'});
  const server = http.createServer(async (req,res) => {
    try {
      const name = path.basename(new URL(req.url,'http://localhost').pathname);
      res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');
      res.end(await fs.readFile(path.join(output,name)));
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise(resolve => server.listen(0,'127.0.0.1',resolve));
  const browser = await chromium.launch({executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const errors=[], exact=[], results=[];
  const dense = process.env.CAD_DRILLED_DENSE === '1';
  try {
    const page = await browser.newPage({viewport:{width:1200,height:900}});
    page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(request.url()))exact.push(request.url());});
    for (const mode of ['preview','gpu','stream']) {
      await page.goto(`http://127.0.0.1:${server.address().port}/plate.${mode}.html`);
      if (mode !== 'preview') await page.waitForFunction(()=>window.cadGpuTest);
      for (const view of ['top','iso']) {
        if (mode !== 'preview') await page.evaluate(async view=>{cadGpuTest.setView(view);await cadGpuTest.whenIdle();cadGpuTest.draw();cadGpuTest.finish();},view);
        else await page.locator(`[data-view="${view}"]`).click();
        results.push(await page.evaluate(({mode,view,outer,holes,circles,height:modelHeight,dense})=>{
          const a=mode==='preview'?yaw: -Math.PI/3, b=mode==='preview'?pitch:Math.PI/5;
          const eye=view==='top'?[0,0,1]:[Math.cos(a)*Math.cos(b),Math.sin(a)*Math.cos(b),Math.sin(b)];
          const rings=[outer,...holes], firstBore=2+rings.reduce((n,r)=>n+r.length,0), margin=.4;
          const cross=(a,b)=>a[0]*b[1]-a[1]*b[0];
          function distance(p,a,b) {
            const dx=b[0]-a[0],dy=b[1]-a[1],t=Math.max(0,Math.min(1,((p[0]-a[0])*dx+(p[1]-a[1])*dy)/(dx*dx+dy*dy)));
            return Math.hypot(p[0]-a[0]-t*dx,p[1]-a[1]-t*dy);
          }
          function inside(p,ring) {
            let value=false;
            for(let i=0,j=ring.length-1;i<ring.length;j=i++) {
              const a=ring[i],b=ring[j];
              if((a[1]>p[1])!==(b[1]>p[1]) && p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0])value=!value;
            }
            return value;
          }
          function oracle(p) {
            const hits=[];let uncertain=false;
            for(const [z,face] of [[0,0],[modelHeight,1]]) {
              const t=(z-p[2])/eye[2],q=[p[0]+t*eye[0],p[1]+t*eye[1]];
              for(const ring of rings)for(let i=0;i<ring.length;i++)if(distance(q,ring[i],ring[(i+1)%ring.length])<margin)uncertain=true;
              for(const c of circles)if(Math.abs(Math.hypot(q[0]-c.center[0],q[1]-c.center[1])-c.radius)<margin)uncertain=true;
              if(inside(q,outer)&&!holes.some(r=>inside(q,r))&&!circles.some(c=>Math.hypot(q[0]-c.center[0],q[1]-c.center[1])<c.radius))hits.push({t,face});
            }
            let face=2;
            for(const ring of rings)for(let i=0;i<ring.length;i++,face++) {
              const a=ring[i],b=ring[(i+1)%ring.length],v=[b[0]-a[0],b[1]-a[1]],den=cross(eye,v);
              if(Math.abs(den)<1e-10)continue;
              const d=[a[0]-p[0],a[1]-p[1]],t=cross(d,v)/den,u=cross(d,eye)/den,z=p[2]+t*eye[2];
              if(u>0&&u<1&&z>0&&z<modelHeight) {
                if(Math.min(u,1-u)*Math.hypot(...v)<margin||Math.min(z,modelHeight-z)<margin)uncertain=true;
                hits.push({t,face});
              }
            }
            const A=eye[0]**2+eye[1]**2;
            if(A>1e-10)for(let i=0;i<circles.length;i++) {
              const c=circles[i],x=p[0]-c.center[0],y=p[1]-c.center[1],B=2*(x*eye[0]+y*eye[1]),C=x*x+y*y-c.radius*c.radius,D=B*B-4*A*C;
              if(Math.abs(Math.abs(cross([x,y],eye))/Math.sqrt(A)-c.radius)<margin)uncertain=true;
              if(D>0)for(const t of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)]) {
                const z=p[2]+t*eye[2];
                if(z>0&&z<modelHeight) {if(Math.min(z,modelHeight-z)<margin)uncertain=true;hits.push({t,face:firstBore+i});}
              }
            }
            return {uncertain,face:hits.sort((a,b)=>b.t-a.t)[0]?.face??null};
          }
          function pick(p) {
            if(mode!=='preview') {const q=cadGpuTest.project(p),hit=cadGpuTest.pick(q[0],q[1]);return hit&&hit.instance>=0?hit.face:null;}
            const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
            const project=q=>{q=sub(q,center);return [dot(q,right),-dot(q,up)];};
            const extent=[0,0];for(const f of scene.faces)for(const v of f.points){const q=project(v);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));}
            const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1)),r=project(p),q=[width/2+r[0]*scale,height/2+20+r[1]*scale];
            if(q[0]<0||q[0]>=width||q[1]<0||q[1]>=height)return null;
            const id=faceBuffer[Math.floor(q[1]*renderRatio)*bufferWidth+Math.floor(q[0]*renderRatio)],f=scene.faces.find(f=>f.id===id);return f?f.local_face_id:null;
          }
          let samples=0,openings=0;const counts={},step=dense?1.13:2.26;
          for(let x=-4.23;x<85;x+=step)for(let y=-4.37;y<75;y+=step) {
            const p=[x,y,modelHeight/2],expected=oracle(p);if(expected.uncertain)continue;
            const actual=pick(p);
            if(actual!==expected.face)throw Error(`${mode}/${view} ray ${JSON.stringify(p)} expected ${expected.face}, got ${actual}`);
            if(actual===null&&inside(p,outer))openings++;
            if(actual!==null)counts[actual]=(counts[actual]||0)+1;samples++;
          }
          if(samples<(dense?2000:700)||openings<(view==='top'?25:dense?10:3)||!(counts[1]>100))throw Error(`Insufficient cap/opening coverage ${JSON.stringify({mode,view,samples,openings,counts})}`);
          if(view==='iso') {
            for(let i=0;i<circles.length;i++)if(!(counts[firstBore+i]>=5))throw Error(`Insufficient bore ${i} wall coverage`);
            if(!(counts[2]>=20&&counts[3]>=10&&counts[7]>=10&&(counts[10]||0)+(counts[11]||0)>=5))throw Error('Insufficient outer, notch or polygon-hole wall coverage');
          }
          return {mode,view,samples,openings,faceSamples:counts};
        },{mode,view,outer,holes,circles,height,dense}));
        console.log(JSON.stringify({completed:results[results.length-1]}));
      }
      await page.locator('#recipe-toggle').click();
      assert.equal(await page.locator('#recipe-source').textContent(),source);
      if(mode==='stream')await page.screenshot({path:'/tmp/cad-drilled-prism.png'});
    }
    assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
    console.log(JSON.stringify({results,exactLoads:0,capturedRecipe:true},null,2));
  } finally {
    await browser.close();await new Promise(resolve=>server.close(resolve));await fs.rm(temporary,{recursive:true,force:true});
  }
})().catch(error=>{console.error(error);process.exitCode=1;});
