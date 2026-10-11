// Independent rotated-box ray/slab oracle for occurrence-aware depth and picking.
// Run: node crates/cad-preview/tests/assembly-check.cjs
const assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {pathToFileURL}=require('node:url');
const http=require('node:http');
const root=path.resolve(__dirname,'../../..');
const {chromium}=require(path.join(root,'tools/preview-check/node_modules/playwright'));
function fixture(){
 const vertices=[[0,0,0],[30,0,0],[30,10,0],[0,10,0],[0,0,10],[30,0,10],[30,10,10],[0,10,10]],rings=[[0,3,2,1],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]],normals=[[0,0,-1],[0,0,1],[0,-1,0],[1,0,0],[0,1,0],[-1,0,0]];
 const edges=new Map(),faces=rings.map((ring,id)=>({id,points:ring.map(i=>vertices[i]),normal:normals[id],edges:ring.map((v,i)=>{const key=[v,ring[(i+1)%4]].sort((a,b)=>a-b).join('/');if(!edges.has(key))edges.set(key,edges.size);return edges.get(key)})}));
 const mesh={faces,crease_edges:[...edges.values()],vertices:8,edges:12,solids:1,bounds:[[0,0,0],[30,10,10]],volume:3000};
 const placements=[{id:'base',part:'beam',angle:0,t:[0,0,0]},{id:'rotated',part:'beam',angle:.72,t:[8,-8,14]},{id:'alias-occurrence',part:'alias',angle:-.48,t:[-8,10,5]}];
 const bounds=[[Infinity,Infinity,Infinity],[-Infinity,-Infinity,-Infinity]];
 for(const item of placements)for(const point of vertices){const c=Math.cos(item.angle),s=Math.sin(item.angle),p=[c*point[0]-s*point[1]+item.t[0],s*point[0]+c*point[1]+item.t[1],point[2]+item.t[2]];p.forEach((v,i)=>{bounds[0][i]=Math.min(bounds[0][i],v);bounds[1][i]=Math.max(bounds[1][i],v)})}
 return {raw:{schema_version:1,kind:'assembly',assembly_id:'fixture',content_hash:'fixture-hash',meshes:{shared:mesh},definitions:{beam:{mesh:'shared',feature_id:'beam-feature'},alias:{mesh:'shared',feature_id:'alias-feature'}},instances:placements.map(p=>({id:p.id,part:p.part,transform:{translation:p.t,rotation:[0,0,Math.sin(p.angle/2),Math.cos(p.angle/2)]}})),bounds,material_volume:9000,solids:3,display_faces:18,display_points:72},placements};
}
(async()=>{
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'cad-assembly-viewer-')),file=path.join(dir,'fixture.preview.html'),{raw,placements}=fixture();
 const recipe={language:'starlark',source:'# shared source retained\nbeam = part(id="beam", feature="beam-feature")',source_name:'fixture.star',output_name:'fixture',feature_id:'fixture',parameters:{},content_hash:'fixture-hash',recipe_context_hash:'source-hash'};
 fs.writeFileSync(file,fs.readFileSync(path.join(root,'crates/cad-preview/src/viewer.html'),'utf8').replace('/*SCENE_DATA*/null',JSON.stringify(raw)).replace('/*RECIPE_DATA*/null',JSON.stringify(recipe)));
 let server;
 const browser=await chromium.launch({headless:true,executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1200,height:900},deviceScaleFactor:2}),errors=[];
  page.on('pageerror',error=>errors.push(error.message));await page.goto(pathToFileURL(file).href);
  const result=await page.evaluate(placements=>{
   const failures=[],seen=new Set();let samples=0,picks=0;
   const product=(a,b)=>a.reduce((s,v,i)=>s+v*b[i],0);
   for(const angle of [-.75,.35,1.2])for(const elevation of [.35,.7,-.35]){
    yaw=angle;pitch=elevation;draw();
    const right=[-Math.sin(yaw),Math.cos(yaw),0],up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)],direction=[Math.cos(yaw)*Math.cos(pitch),Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch)];
    const extent=[0,0];for(const face of scene.faces)for(const point of face.points){const p=point.map((v,i)=>v-center[i]);extent[0]=Math.max(extent[0],Math.abs(product(p,right)));extent[1]=Math.max(extent[1],Math.abs(product(p,up)))}
    const scale=zoom*Math.min(Math.max(1,width-80)/(2*extent[0]||1),Math.max(1,height-150)/(2*extent[1]||1));
    for(let py=3;py<bufferHeight;py+=13)for(let px=3;px<bufferWidth;px+=13){
     const x=((px+.5)/renderRatio-width/2)/scale,y=-((py+.5)/renderRatio-height/2-20)/scale,origin=center.map((v,i)=>v+right[i]*x+up[i]*y),hits=[];
     for(const p of placements){
      const c=Math.cos(p.angle),s=Math.sin(p.angle),o=origin.map((v,i)=>v-p.t[i]),local=[c*o[0]+s*o[1],-s*o[0]+c*o[1],o[2]],ray=[c*direction[0]+s*direction[1],-s*direction[0]+c*direction[1],direction[2]];
      let lo=-Infinity,hi=Infinity,face=-1;
      for(let axis=0;axis<3;axis++){
       if(Math.abs(ray[axis])<1e-12){if(local[axis]<0||local[axis]>[30,10,10][axis]){lo=Infinity;break}continue}
       const a=-local[axis]/ray[axis],b=([30,10,10][axis]-local[axis])/ray[axis];lo=Math.max(lo,Math.min(a,b));
       if(Math.max(a,b)<hi){hi=Math.max(a,b);face=ray[axis]>0?[3,4,1][axis]:[5,2,0][axis]}
      }
      if(hi-lo>1e-6)hits.push({id:p.id,face,depth:hi});
     }
     if(!hits.length)continue;hits.sort((a,b)=>b.depth-a.depth);if(hits.length>1&&Math.abs(hits[0].depth-hits[1].depth)<1e-6)continue;
     const expected=hits[0],actual=scene.faces[faceBuffer[py*bufferWidth+px]];samples++;
     if(!actual||actual.instance_id!==expected.id||actual.local_face_id!==expected.face){if(failures.length<8)failures.push({expected,actual,px,py});continue}
     if(!seen.has(expected.id)){select((px+.5)/renderRatio,(py+.5)/renderRatio);if(selectedInstance!==expected.id)throw Error('Picking occurrence mismatch');if(!document.getElementById('selection').textContent.includes('Definition: '+actual.part_id))throw Error('Missing definition identity');if(!document.getElementById('recipe-summary').textContent.includes('Feature: '+actual.feature_id))throw Error('Incorrect selected feature context');seen.add(expected.id);picks++}
    }
   }
   return {samples,picks,seen:[...seen],failures,uniqueMeshes:Object.keys(compactScene.meshes).length,faces:scene.faces.length};
  },placements);
  assert.deepEqual(errors,[]);assert.deepEqual(result.failures,[]);assert.ok(result.samples>1000);assert.equal(result.picks,3);assert.equal(result.uniqueMeshes,1);assert.equal(result.faces,18);
  await page.locator('#occurrence-select').selectOption('alias-occurrence');
  assert.match(await page.locator('#recipe-summary').textContent(),/alias-feature/);
  assert.equal(await page.locator('#recipe-source').textContent(),recipe.source);
  await page.setViewportSize({width:390,height:844});assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  // Exercise compact assembly refresh through the same manifest/HTML contract.
  let currentHtml=fs.readFileSync(file,'utf8');
  let manifest={outputs:{fixture:{content_hash:recipe.content_hash}},recipe_context_hash:recipe.recipe_context_hash};
  server=http.createServer((request,response)=>{response.setHeader('Cache-Control','no-store');response.setHeader('Content-Type',request.url==='/manifest.json'?'application/json':'text/html');response.end(request.url==='/manifest.json'?JSON.stringify(manifest):currentHtml)});
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
  const live=await browser.newPage({viewport:{width:1200,height:900}});live.on('pageerror',error=>errors.push(error.message));
  await live.goto(`http://127.0.0.1:${server.address().port}/fixture.preview.html`);
  await live.waitForFunction(()=>document.querySelector('.badge').textContent==='LIVE PREVIEW');
  const nextRaw=structuredClone(raw),nextRecipe={...recipe,source:recipe.source+'\n# moved occurrence',content_hash:'fixture-v2',recipe_context_hash:'source-v2'};
  nextRaw.instances[1].transform.translation[2]+=7;nextRaw.bounds[1][2]+=7;nextRaw.content_hash='fixture-v2';
  manifest={outputs:{fixture:{content_hash:nextRecipe.content_hash}},recipe_context_hash:nextRecipe.recipe_context_hash};
  await live.waitForFunction(()=>document.querySelector('.badge').textContent==='RECONNECTING');
  assert.equal(await live.evaluate(()=>compactScene.instances[1].transform.translation[2]),14,'mismatched generation must preserve old scene');
  const template=fs.readFileSync(path.join(root,'crates/cad-preview/src/viewer.html'),'utf8');
  const render=(data,context)=>template.replace('/*SCENE_DATA*/null',JSON.stringify(data)).replace('/*RECIPE_DATA*/null',JSON.stringify(context));
  currentHtml=render(nextRaw,nextRecipe);
  await live.waitForFunction(()=>compactScene.instances[1].transform.translation[2]===21);
  assert.equal(await live.evaluate(()=>Object.keys(compactScene.meshes).length),1);
  assert.equal(await live.evaluate(()=>scene.faces.length),18);
  await live.locator('#occurrence-select').selectOption('rotated');
  const commentRecipe={...nextRecipe,source:nextRecipe.source+'\n# comment only',recipe_context_hash:'source-v3'};
  manifest.recipe_context_hash=commentRecipe.recipe_context_hash;currentHtml=render(nextRaw,commentRecipe);
  await live.waitForFunction(()=>recipeContext.source.includes('# comment only'));
  assert.equal(await live.evaluate(()=>selectedInstance),'rotated','source-only edits preserve occurrence selection');
  assert.deepEqual(errors,[]);
  result.compactRefresh=true;result.failedGenerationPreserved=true;result.sourceOnlySelectionPreserved=true;
  console.log(JSON.stringify(result));
 }finally{await browser.close();if(server)await new Promise(resolve=>server.close(resolve));fs.rmSync(dir,{recursive:true,force:true})}
})().catch(error=>{console.error(error);process.exit(1)});
