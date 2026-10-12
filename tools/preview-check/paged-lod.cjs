// Curved geometry refinement with paged metadata; local functional acceptance.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const url=process.env.CAD_PAGED_LOD_URL||'http://localhost:8080/paged_spindles/spindle_line.paged.html';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000},deviceScaleFactor:1});
  const errors=[],requests=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>requests.push(r.url()));
  await page.goto(url);await page.waitForFunction(()=>window.cadPagedStats?.ready);
  const camera=async halfHeight=>page.evaluate(h=>{cadPaged.setView('top');cadPaged.setCamera({target:[0,0,30],halfHeight:h});},halfHeight);
  const settle=()=>page.evaluate(()=>cadPaged.settle());
  const snapshot=()=>page.evaluate(()=>cadPaged.snapshot());
  await camera(500);await settle();
  const coarse=await snapshot();
  assert.ok(Object.keys(coarse.stats.activeLodLevels).length>0);
  assert.ok(Object.values(coarse.stats.activeLodLevels).every(n=>n===0));
  assert.equal(coarse.stats.unmetDetailCount,0);
  const fines=new Set(coarse.metadata.entries.flatMap(e=>e.definition.mesh_lods.length>1?[e.definition.mesh_lods.at(-1).url]:[]));
  let release;const gate=new Promise(resolve=>release=resolve);let held=0;
  await page.route('**/mesh-*.display.json',async route=>{
   if(fines.has(new URL(route.request().url()).pathname.split('/').at(-1))){held++;await gate;}
   await route.continue();
  });
  await camera(30);
  await page.waitForFunction(()=>cadPagedStats.reservedMeshBytes>0);
  const pending=await snapshot();
  assert.ok(held>0);
  assert.equal(pending.stats.proxyCount,0,'verified coarse geometry stays visible during refinement');
  assert.ok(pending.stats.unmetDetailCount>0);
  const coarseHit=await page.evaluate(()=>cadPaged.pick(...cadPaged.project([12,0,45])));
  assert.deepEqual(coarseHit.path,['station_0','retainer_0']);
  assert.equal(coarseHit.face,1);
  release();await settle();
  const fine=await snapshot();
  assert.ok(Object.values(fine.stats.activeLodLevels).every(n=>n===2));
  assert.equal(fine.stats.unmetDetailCount,0);assert.equal(fine.stats.proxyCount,0);
  assert.ok(fine.stats.maxScreenErrorPixels<=1);
  const fineHit=await page.evaluate(()=>cadPaged.pick(...cadPaged.project([12,0,45])));
  assert.deepEqual(fineHit,coarseHit,'native face and occurrence survive mesh replacement');
  // Explicit resource reduction may require proxies; never claim met detail.
  await page.evaluate(()=>cadPaged.setBudget(1024));await settle();
  const pressure=await snapshot();
  assert.ok(pressure.stats.residentMeshBytes+pressure.stats.reservedMeshBytes<=1024);
  assert.ok(pressure.stats.unmetDetailCount>0);assert.ok(pressure.stats.proxyCount>0);
  await page.evaluate(()=>cadPaged.setBudget(64*1048576));await settle();
  const restored=await snapshot();
  assert.equal(restored.stats.unmetDetailCount,0);assert.equal(restored.stats.proxyCount,0);
  assert.deepEqual(await page.evaluate(()=>cadPaged.pick(...cadPaged.project([12,0,45]))),fineHit);
  await page.screenshot({path:'/tmp/cad-paged-lod.png'});
  const limited=await browser.newPage({viewport:{width:1440,height:1000}});
  await limited.goto(new URL('service_station.paged.html',url).href);
  await limited.waitForFunction(()=>window.cadPagedStats?.ready);
  await limited.evaluate(async()=>{cadPaged.setView('top');cadPaged.setCamera({target:[0,0,30],halfHeight:250});await cadPaged.settle();});
  assert.ok(await limited.evaluate(()=>cadPagedStats.residentMeshBytes>0),'coarse surfaces must exist before constraining refinement');
  const allowance=await limited.evaluate(()=>cadPagedStats.residentMeshBytes+1024);
  await limited.evaluate(async allowance=>{cadPaged.setBudget(allowance);cadPaged.setCamera({halfHeight:30});await cadPaged.settle();},allowance);
  const retained=await limited.evaluate(()=>cadPaged.snapshot());
  assert.equal(retained.stats.proxyCount,0,'a refinement budget failure must not discard current surfaces');
  assert.ok(retained.stats.unmetDetailCount>0);
  assert.ok(retained.stats.residentMeshBytes+retained.stats.reservedMeshBytes<=allowance);
  await limited.evaluate(async()=>{cadPaged.setBudget(64*1048576);await cadPaged.settle();});
  assert.equal(await limited.evaluate(()=>cadPagedStats.unmetDetailCount),0);
  await limited.evaluate(async()=>{cadPaged.setCamera({target:[12,0,45],halfHeight:0.01});await cadPaged.settle();});
  const precisionLimited=await limited.evaluate(()=>cadPaged.snapshot());
  assert.ok(precisionLimited.stats.unmetDetailCount>0);
  assert.ok(Object.values(precisionLimited.stats.unmetDetailReasons).includes('display precision floor'));
  for(const s of [coarse,pending,fine,pressure,restored]){
   assert.ok(s.metadata.stats.residentRecords<=2048);
   assert.ok(s.metadata.stats.residentBytes+s.metadata.stats.reservedBytes<=8*1048576);
   assert.ok(s.stats.residentMeshBytes+s.stats.reservedMeshBytes<=64*1048576);
   assert.deepEqual(s.stats.errors,[]);assert.deepEqual(s.metadata.stats.errors,[]);
  }
  assert.deepEqual(errors,[]);
  assert.equal(requests.filter(u=>/\.(brep\.json|step|stl)(?:$|\?)/.test(u)).length,0);
  console.log(JSON.stringify({valid:true,url,coarse:coarse.stats,pending:pending.stats,fine:fine.stats,pressure:pressure.stats,retained:retained.stats,precisionLimited:precisionLimited.stats,restored:restored.stats,pick:fineHit,requests:requests.length,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1});
