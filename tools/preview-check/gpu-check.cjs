// GPU correctness, not a hardware performance gate. Explicit SwiftShader makes
// this suite reproducible on CI without depending on an installed GPU driver.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const { execFile } = require('node:child_process');
const { promisify } = require('node:util');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const execute = promisify(execFile);
const root = path.resolve(__dirname, '../..');
const binary = process.env.CAD_BIN || path.join(root, 'target/debug/cad');
const source = `# Literal hostile source remains text: </script><img src=x onerror="window.cadInjected=true">
size = param(name = "__proto__", default = 8)
p = part(id = "__proto__", body = extrude(id = "box", profile = rectangle(id = "profile", width = size, depth = 5), height = 3))
a = instance(id = "__proto__", part = p)
b = instance(id = "tilted", part = p, offset = [3, 2, 4], rotation = [0.3826834323650898, 0, 0, 0.9238795325112867])
c = assembly(id = "module", instances = [a, b])
n = assembly_instance(id = "rotated", assembly = c, offset = [5, -2, 1], rotation = [0, 0, 0.25881904510252074, 0.9659258262890683])
f = instance(id = "front", part = p, offset = [2, 1, 10], rotation = [0, 0.17364817766693033, 0, 0.984807753012208])
assembly_output(name = "boxes", assembly = assembly(id = "boxes", instances = [n, f]))
t = part(id = "triangle", body = extrude(id = "triangle_body", profile = polygon(id = "triangle_profile", points = [[0,0], [40,0], [0,20]]), height = 4))
assembly_output(name = "triangle", assembly = assembly(id = "triangle_assembly", instances = [instance(id = "triangle_instance", part = t)]))
thin = part(id = "thin", body = extrude(id = "thin_body", profile = rectangle(id = "thin_profile", width = 10000, depth = 10000), height = 0.001))
assembly_output(name = "thin", assembly = assembly(id = "thin_assembly", instances = [instance(id = "thin_instance", part = thin)]))
`;
let browser, temporary;
(async () => {
  temporary = await fs.mkdtemp(path.join(os.tmpdir(), 'cad-gpu-check-'));
  const recipe = path.join(temporary, 'oracle.star');
  const output = path.join(temporary, 'small');
  await fs.writeFile(recipe, source);
  await execute(binary, ['build', recipe, '--display-only', '--param', '__proto__=8', '--out-dir', output], { maxBuffer: 8 * 1024 * 1024 });
  const hugeRecipe=path.join(temporary,'huge.star'),huge=path.join(temporary,'huge');
  await fs.writeFile(hugeRecipe,'r=rectangle(id="r",width=1e50,depth=1e50)\ne=extrude(id="e",profile=r,height=1e50)\np=part(id="p",body=e)\nassembly_output(name="huge",assembly=assembly(id="huge",instances=[instance(id="huge",part=p)]))\n');
  await execute(binary,['build',hugeRecipe,'--display-only','--out-dir',huge],{maxBuffer:8*1024*1024});
  const large = path.join(temporary, 'large');
  await execute(binary, ['build', path.join(root, 'examples/display_rack.star'), '--display-only', '--param', 'rows=100', '--param', 'columns=100', '--param', 'variants=1', '--out-dir', large], { maxBuffer: 32 * 1024 * 1024 });
  browser = await chromium.launch({ headless: true, executablePath: process.env.CAD_BROWSER || undefined,
    args: ['--no-sandbox', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'] });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 }, deviceScaleFactor: 1 });
  const errors = [], exactRequests = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('request', request => { if (/\.(brep\.json|step|stl)(?:$|\?)/.test(request.url())) exactRequests.push(request.url()); });
  async function open(file) {
    await page.goto(pathToFileURL(file).href);
    await page.waitForFunction(() => window.cadGpuStats?.ready || window.cadGpuStats?.errors?.length);
    const stats = await page.evaluate(() => window.cadGpuStats);
    assert.equal(stats.ready, true, JSON.stringify(stats));
    assert.deepEqual(stats.errors, []);
  }
  await open(path.join(output, 'boxes.gpu.html'));
  await page.getByRole('button', { name: 'Recipe', exact: true }).click();
  assert.equal(await page.locator('#recipe-source').textContent(), source);
  const boxes = await page.evaluate(expectedSource => {
    const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
    if (!own(compactScene.definitions, '__proto__') || !own(recipeContext.parameters, '__proto__')) throw Error('Prototype-shaped data key lost');
    if (recipeContext.source !== expectedSource || recipeContext.parameters.__proto__ !== 8 || window.cadInjected) throw Error('Source snapshot or escaping failure');
    // Independent transforms from authored recipe, not renderer transform arrays.
    const rz = [[Math.cos(Math.PI/6), -Math.sin(Math.PI/6), 0], [Math.sin(Math.PI/6), Math.cos(Math.PI/6), 0], [0,0,1]];
    const rx = [[1,0,0], [0,Math.SQRT1_2,-Math.SQRT1_2], [0,Math.SQRT1_2,Math.SQRT1_2]];
    const ry = [[Math.cos(Math.PI/9),0,Math.sin(Math.PI/9)], [0,1,0], [-Math.sin(Math.PI/9),0,Math.cos(Math.PI/9)]];
    const mul = (a,b) => a.map(row => b[0].map((_,j) => row.reduce((s,v,k) => s+v*b[k][j],0)));
    const mv = (a,v) => a.map(row => row.reduce((s,n,j) => s+n*v[j],0));
    const add = (a,b) => a.map((v,i) => v+b[i]);
    const models = [
      {id:JSON.stringify(['rotated','__proto__']), r:rz,t:[5,-2,1]},
      {id:JSON.stringify(['rotated','tilted']), r:mul(rz,rx),t:add([5,-2,1],mv(rz,[3,2,4]))},
      {id:JSON.stringify(['front']),r:ry,t:[2,1,10]},
    ];
    const max = [8,5,3];
    function hit(model, origin, direction) {
      const tr = model.r[0].map((_,j) => model.r.map(row => row[j]));
      const o = mv(tr, origin.map((v,i) => v-model.t[i])), d = mv(tr,direction);
      let near = -Infinity, far = Infinity, axis = -1, sign;
      for (let a=0;a<3;a++) {
        if (Math.abs(d[a])<1e-10) { if (o[a]<0||o[a]>max[a]) return null; continue; }
        let x=-o[a]/d[a], y=(max[a]-o[a])/d[a], s=-1;
        if (x>y) { [x,y]=[y,x];s=1; }
        if(x>near) { near=x;axis=a;sign=s; } far=Math.min(far,y);
      }
      if(near>far||far<0) return null;
      const p=o.map((v,i)=>v+near*d[i]);
      const boundary=p.some((v,i)=>i!==axis&&Math.min(Math.abs(v),Math.abs(v-max[i]))<.025);
      return {distance:near,axis,sign,boundary,model};
    }
    let samples=0,hits=0,misses=0,occlusions=0;
    const selected=new Set();
    for(const [view,axes,direction] of [['top',[0,1],[0,0,-1]],['front',[0,2],[0,1,0]],['right',[1,2],[-1,0,0]]]) {
      cadGpuTest.setView(view);cadGpuTest.fit();cadGpuTest.render();
      // Recover the screen-to-world affine map for this canonical view only;
      // the ray and box slab tests remain independent of rendering code.
      const zero=cadGpuTest.project([0,0,0]);
      const basis=axes.map(a=>{const p=[0,0,0];p[a]=1;return cadGpuTest.project(p).map((v,i)=>v-zero[i]);});
      const det=basis[0][0]*basis[1][1]-basis[1][0]*basis[0][1];
      for(let a=-2.3;a<19;a+=1.3)for(let b=-4.7;b<15;b+=1.3){
        const p=[0,0,0];p[axes[0]]=a;p[axes[1]]=b;
        const screen=cadGpuTest.project(p);const x=Math.floor(screen[0])+.5,y=Math.floor(screen[1])+.5;
        if(x<1||y<1||x>=viewWidth() -1||y>=viewHeight()-1)continue;
        const dx=x-zero[0],dy=y-zero[1];
        const o=[0,0,0];o[axes[0]]=(dx*basis[1][1]-dy*basis[1][0])/det;o[axes[1]]=(dy*basis[0][0]-dx*basis[0][1])/det;
        const rayAxis=direction.findIndex(v=>v!==0);o[rayAxis]=-direction[rayAxis]*1000;
        const intersections=models.map(m=>hit(m,o,direction)).filter(Boolean).sort((a,b)=>a.distance-b.distance);
        if(intersections.some(h=>h.boundary)||intersections.length>1&&Math.abs(intersections[0].distance-intersections[1].distance)<.025)continue;
        const expected=intersections[0],actual=cadGpuTest.pick(x,y);
        if(!expected){if(actual!==null)throw Error(`${view} false hit at ${x},${y}: ${JSON.stringify(actual)}`);misses++;}
        else {
          if(!actual)throw Error(`${view} missed ${expected.model.id} at ${x},${y}`);
          const occurrence=compactScene.instances[actual.instance];
          if(occurrence.id!==expected.model.id)throw Error(`${view} depth mismatch expected ${expected.model.id} got ${occurrence.id}`);
          const mesh=compactScene.meshes[compactScene.definitions[occurrence.part].mesh];
          const face=mesh.faces.find(f=>f.id===actual.face);
          if(!face||face.normal.some((v,i)=>Math.abs(v-(i===expected.axis?expected.sign:0))>1e-8))throw Error(`${view} wrong local face ${actual.face}`);
          selected.add(occurrence.id);hits++;if(intersections.length>1)occlusions++;
        }
        samples++;
      }
    }
    function viewWidth(){return document.querySelector('#view').getBoundingClientRect().width;}
    function viewHeight(){return document.querySelector('#view').getBoundingClientRect().height;}
    return {samples,hits,misses,occlusions,occurrences:[...selected],stats:cadGpuStats};
  },source);
  assert.ok(boxes.hits>40 && boxes.misses>40 && boxes.occlusions>5,JSON.stringify(boxes));
  assert.equal(boxes.occurrences.length,3);
  assert.equal(boxes.stats.meshCount,1);
  await open(path.join(output,'triangle.gpu.html'));
  const triangle=await page.evaluate(()=>{
    cadGpuTest.setView('top');cadGpuTest.fit();cadGpuTest.render();
    const mesh=Object.values(compactScene.meshes)[0],cap=mesh.faces.find(f=>f.normal[2]===1);
    let inside=0,outside=0,visiblePixels=0;
    for(let x=-2;x<=42;x+=2)for(let y=-2;y<=22;y+=2){
      if(Math.abs(x)<.5||Math.abs(y)<.5||Math.abs(x/40+y/20-1)<.06)continue;
      const [sx,sy]=cadGpuTest.project([x,y,4]);
      const expected=x>0&&y>0&&x/40+y/20<1,actual=cadGpuTest.pick(sx,sy);
      if(expected?actual?.face!==cap.id:actual!==null)throw Error(`Triangle silhouette mismatch ${x},${y}: ${JSON.stringify(actual)}`);
      if(expected){
        inside++;
        const canvas=document.getElementById('view'),gl=canvas.getContext('webgl2'),rect=canvas.getBoundingClientRect(),rgba=new Uint8Array(4);
        gl.readPixels(Math.floor(sx*canvas.width/rect.width),canvas.height-1-Math.floor(sy*canvas.height/rect.height),1,1,gl.RGBA,gl.UNSIGNED_BYTE,rgba);
        if(rgba[3]!==255||Math.abs(rgba[0]-234)+Math.abs(rgba[1]-240)+Math.abs(rgba[2]-236)<30)throw Error('Picked triangle is blank in visible framebuffer');
        visiblePixels++;
      }else outside++;
    }
    return {inside,outside,visiblePixels};
  });
  assert.ok(triangle.inside>20&&triangle.outside>20);
  await open(path.join(output,'thin.gpu.html'));
  const thinPlate=await page.evaluate(()=>{
    cadGpuTest.setView('top');cadGpuTest.fit();
    const mesh=Object.values(compactScene.meshes)[0],cap=mesh.faces.find(face=>face.normal[2]===1);
    const canvas=document.getElementById('view'),gl=canvas.getContext('webgl2');
    let topHits=0,background=0;
    // XY shifts exercise panning; either sign of the eye-axis target shift
    // must leave the geometry within the depth clipping interval.
    for(const target of [[5000,5000,.0005],[6400,4200,2],[3600,5800,-2]]){
      cadGpuTest.setCamera({target});cadGpuTest.render();
      for(const world of [[2000,2000,.001],[5000,5000,.001],[8000,8000,.001],[-1000,5000,.001],[11000,5000,.001]]){
        const [x,y,depth]=cadGpuTest.project(world),hit=cadGpuTest.pick(x,y);
        const inside=world[0]>0&&world[0]<10000;
        if(inside){
          if(hit?.instance!==0||hit.face!==cap.id)throw Error(`Thin plate top lost to bottom or clipping at target ${target}: ${JSON.stringify(hit)}`);
          if(Math.abs(depth)>=1)throw Error('Thin plate projected outside clipping interval');
          topHits++;
        }else{if(hit!==null)throw Error('Thin plate background incorrectly pickable');background++;}
        const rect=canvas.getBoundingClientRect(),rgba=new Uint8Array(4);
        gl.readPixels(Math.floor(x*canvas.width/rect.width),canvas.height-1-Math.floor(y*canvas.height/rect.height),1,1,gl.RGBA,gl.UNSIGNED_BYTE,rgba);
        const difference=Math.abs(rgba[0]-234)+Math.abs(rgba[1]-240)+Math.abs(rgba[2]-236);
        if(rgba[3]!==255||(inside?difference<30:difference>6))throw Error('Thin plate visible framebuffer disagrees with analytic silhouette');
      }
    }
    return {width_to_thickness:10000000,topHits,background,shifted_targets:3};
  });
  assert.equal(thinPlate.topHits,9);
  assert.equal(thinPlate.background,6);
  await open(path.join(large,'display_rack.gpu.html'));
  const shared=await page.evaluate(()=>({stats:cadGpuStats,localFaces:Object.values(compactScene.meshes).reduce((n,m)=>n+m.faces.length,0),localPoints:Object.values(compactScene.meshes).reduce((n,m)=>n+m.faces.reduce((sum,f)=>sum+f.points.length,0),0),expandedFaces:Object.hasOwn(compactScene,'faces')}));
  assert.equal(shared.stats.instanceCount,10000);
  assert.equal(shared.stats.meshCount,1);
  assert.equal(shared.stats.geometryCopies,1);
  assert.equal(shared.localFaces,6);
  assert.equal(shared.localPoints,24);
  assert.equal(shared.expandedFaces,false);
  assert.ok(shared.stats.geometryVertices<=36);
  assert.ok(shared.stats.instanceBufferBytes<=10000*128);
  assert.ok(shared.stats.drawCalls<=4);
  assert.equal(shared.stats.cpuExpandedFaces,0);
  const largePicks=await page.evaluate(()=>{
    cadGpuTest.setView('top');cadGpuTest.fit();cadGpuTest.render();
    return [[0,0],[25,50],[99,99]].map(([row,column])=>{
      const id=`cell_${row}_${column}`,index=compactScene.instances.findIndex(instance=>instance.id===id);
      const [x,y]=cadGpuTest.project([12*column+4,12*row+4,8]);
      const hit=cadGpuTest.pick(x,y);
      if(hit?.instance!==index)throw Error(`Large occurrence ID mismatch ${id}: ${JSON.stringify(hit)}`);
      const occurrence=compactScene.instances[index],mesh=compactScene.meshes[compactScene.definitions[occurrence.part].mesh];
      if(mesh.faces.find(f=>f.id===hit.face)?.normal[2]!==1)throw Error(`Wrong top face for ${id}`);
      return {id,index,face:hit.face};
    });
  });
  assert.ok(largePicks.some(pick=>pick.index>255));
  assert.deepEqual(exactRequests,[]);
  assert.deepEqual(errors,[]);
  await page.goto(pathToFileURL(path.join(huge,'huge.gpu.html')).href);
  await page.waitForFunction(()=>window.cadGpuStats?.errors?.length);
  const numeric=await page.evaluate(()=>({ready:cadGpuStats.ready,error:document.getElementById('error').textContent}));
  assert.equal(numeric.ready,false);
  assert.match(numeric.error,/float32 range/);
  const unavailable=await browser.newPage();
  await unavailable.addInitScript(()=>{const get=HTMLCanvasElement.prototype.getContext;HTMLCanvasElement.prototype.getContext=function(kind,...args){return kind==='webgl2'?null:get.call(this,kind,...args);};});
  await unavailable.goto(pathToFileURL(path.join(output,'boxes.gpu.html')).href);
  await unavailable.waitForFunction(()=>document.body.innerText.match(/WebGL2.*(unavailable|not available|unsupported|not supported)/i));
  assert.equal(await unavailable.evaluate(()=>window.cadGpuStats?.ready),false);
  console.log(JSON.stringify({backend:'Chromium ANGLE SwiftShader',boxes,triangle,thinPlate,shared,largePicks,numeric_range_rejected:true,explicit_unsupported:true,exact_brep_requests:exactRequests.length}));
})().catch(error=>{console.error(error);process.exitCode=1;}).finally(async()=>{if(browser)await browser.close();if(temporary)await fs.rm(temporary,{recursive:true,force:true});});
