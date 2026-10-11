// Fast deterministic numeric regression of the actual production hierarchy.
// No WebGL or copied BVH implementation; the oracle is the former OBB slab scan.
const fs=require('node:fs'),vm=require('node:vm'),assert=require('node:assert/strict'),path=require('node:path');
const source=fs.readFileSync(path.join(__dirname, '../../crates/cad-preview/src/gpu-viewer.html'),'utf8');
for (const marker of ['  const occurrenceBounds =', '  const vertex =', '  function cull() {', '  const charge =']) assert(source.includes(marker), `Missing production section: ${marker}`);
const tree=source.slice(source.indexOf('  const occurrenceBounds ='),source.indexOf('  const vertex ='));
const cull=source.slice(source.indexOf('  function cull() {'),source.indexOf('  const charge ='));
let seed=9127; const rng=()=>((seed=(Math.imul(seed,1664525)+1013904223)>>>0)/2**32);
let cases=0, skipped=0, total=0;
const configurations = [1e-12,1,1e12,1e100].flatMap(scale => [0,1,1e8].map(offsetScale => ({scale,offsetScale,mode:'random'})));
configurations.push({scale:1,offsetScale:0,mode:'empty'}, {scale:1,offsetScale:0,mode:'coincident'}, {scale:1,offsetScale:0,mode:'tangent'});
for(const {scale,offsetScale,mode} of configurations) {
 const offset=offsetScale*scale, records=[];
 for(let index=0;index<1024;index++) {
  const theta=rng()*Math.PI*2,c=Math.cos(theta),s=Math.sin(theta);
  records.push({index,hash:'mesh',center:[offset+(rng()-.5)*100*scale,offset+(rng()-.5)*100*scale,offset+(rng()-.5)*100*scale],half:[rng()*scale,rng()*scale,1e-5*scale],axes:[[c,s,0],[-s,c,0],[0,0,1]],empty:index%71===0});
 }
 if (mode==='empty') records.forEach(record => { record.empty=true; });
 if (mode==='coincident') records.forEach(record => { record.center=[0,0,0]; record.half=[1,1,1]; });
 if (mode==='tangent') records.forEach((record,index) => {
  record.empty=false; record.axes=[[1,0,0],[0,1,0],[0,0,1]];record.half=[1,1,1];
  record.center=[(index%2?1:-1)*(2+(index%5-2)*Number.EPSILON),0,0];
 });
 const context={records,instances:records,stats:{bvhNodeCount:0,bvhLeafCount:0,bvhMaxDepth:0},Number,Math,Map,radius:100*scale,finite3:x=>Array.isArray(x)&&x.length===3&&x.every(Number.isFinite),sub:(a,b)=>a.map((x,i)=>x-b[i]),dot:(a,b)=>a.reduce((n,x,i)=>n+x*b[i],0),visible:new Map(),visibleIndices:[],camera:{target:[offset,offset,offset],halfHeight:scale},basis:{right:[1,0,0],up:[0,1,0],eye:[0,0,1]},aspect:1,depthRange:200*scale};
 vm.createContext(context);vm.runInContext(tree+cull,context);
 for(let j=0;j<200;j++) {
  const yaw=rng()*6,pitch=rng()*3-1.5,c=Math.cos(yaw),s=Math.sin(yaw),cp=Math.cos(pitch),sp=Math.sin(pitch);
  context.basis={right:[-s,c,0],up:[-sp*c,-sp*s,cp],eye:[cp*c,cp*s,sp]};
  context.camera={target:[offset+(rng()-.5)*140*scale,offset+(rng()-.5)*140*scale,offset+(rng()-.5)*140*scale],halfHeight:scale*(.01+rng()*30)};
  if (mode==='tangent') {
   context.basis={right:[1,0,0],up:[0,1,0],eye:[0,0,1]};
   context.camera={target:[(j-100)*Number.EPSILON,0,0],halfHeight:1-context.radius*1e-12};
  }
  vm.runInContext('cull()',context);
  const planes=[[context.basis.right,context.camera.halfHeight],[context.basis.up,context.camera.halfHeight],[context.basis.eye,context.depthRange]];
  const expected=records.filter(record=>!record.empty&&planes.every(([axis,extent])=>Math.abs(context.dot(context.sub(record.center,context.camera.target),axis))<=extent+record.axes.reduce((sum,a,i)=>sum+Math.abs(context.dot(axis,a))*record.half[i],0)+context.radius*1e-12)).map(x=>x.index);
  assert.deepEqual([...context.visibleIndices],expected, `${mode}, scale=${scale}, offset ratio=${offsetScale}, camera=${j}`);
  assert(context.stats.cullOccurrenceTests <= records.length);
  if (mode==='empty') assert.equal(context.stats.bvhNodeCount,0);
  cases++;total+=records.length;skipped+=records.length-context.stats.cullOccurrenceTests;
 }
}
console.log(JSON.stringify({cases,occurrenceComparisons:total,occurrenceTestsSkipped:skipped}));
