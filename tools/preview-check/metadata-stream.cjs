// Independent consumer acceptance: no browser, CAD kernel, or geometry fetches.
const assert = require('node:assert/strict');
const {createHash, webcrypto} = require('node:crypto');
globalThis.crypto ??= webcrypto;
globalThis.location ??= new URL('http://localhost:8080/fixture/model.paged.html');
const {CadMetadataStream} = require('../../crates/cad-preview/src/metadata-stream.js');
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(check) {
  for (let i = 0; i < 300; i++) { if (check()) return; await sleep(10); }
  throw new Error('scheduler did not reach expected state');
}
function fixture(duplicate = false) {
  const assets = new Map();
  function page(value, bounds, occurrences) {
    const content = JSON.stringify({schema_version:1, ...value});
    const sha256 = createHash('sha256').update(content).digest('hex');
    const url = `metadata-${sha256}.display.json`;
    assets.set(url, content);
    return {url,sha256,byte_length:Buffer.byteLength(content),bounds,occurrences};
  }
  function entry(index, x) {
    const sha256 = 'a'.repeat(64);
    const mesh = {url:`mesh-${sha256}.display.json`,sha256,byte_length:256,bounds:[[0,0,0],[1,1,1]],faces:6,points:24,vertices:8,gpu_bytes:1008,geometric_error_mm:0};
    return {index,empty:false,instance:{id:`part${index}`,part:'box',transform:{translation:[x,0,0],rotation:[0,0,0,1]}},path:['station',`part${index}`],definition:{mesh:'native-box',feature_id:'box',metadata:{},datums:{},mesh_asset:mesh,mesh_lods:[mesh]}};
  }
  const left = page({kind:'assembly_metadata_leaf',entries:[entry(0,0),entry(1,2)]},[[0,0,0],[3,1,1]],2);
  const right = page({kind:'assembly_metadata_leaf',entries:[entry(duplicate?0:2,100),entry(3,102)]},[[100,0,0],[103,1,1]],2);
  const root = page({kind:'assembly_metadata_branch',children:[left,right]},[[0,0,0],[103,1,1]],4);
  const index = {schema_version:1,kind:'assembly_metadata_index',assembly_id:'fixture',content_hash:'geometry',recipe_context_hash:'source',source_bundle_hash:null,total_occurrences:4,root};
  return {assets,index,left,right};
}
function camera(side) {
  return {intersects:b => side === 'left' ? b[0][0]<50 : b[1][0]>50,priority:()=>1,refine:()=>true};
}
async function run() {
  const f = fixture(), requests=[];
  const fetcher = async url => {const name=new URL(url,location).pathname.split('/').at(-1); requests.push(name); return new Response(f.assets.get(name));};
  const stream = new CadMetadataStream(f.index,{fetcher,maxRecords:2,maxRequests:1,budgetBytes:8192});
  stream.update({intersects:()=>true,priority:()=>1,refine:()=>false});
  await sleep(30);
  assert.equal(requests.length,0,'overview can retain root proxy without decoding leaves');
  assert.equal(stream.snapshot().proxies.length,1);
  stream.update(camera('left'));
  await until(()=>stream.snapshot().entries.length===2);
  assert.deepEqual(stream.snapshot().entries.map(e=>e.index).sort(),[0,1]);
  assert.ok(!requests.includes(f.right.url),'offscreen sibling was never fetched');
  stream.update(camera('right'));
  await until(()=>stream.snapshot().entries.some(e=>e.index===2));
  assert.deepEqual(stream.snapshot().entries.map(e=>e.index).sort(),[2,3]);
  const stats=stream.snapshot().stats;
  assert.ok(stats.residentRecords<=2);
  assert.ok(stats.residentBytes+stats.reservedBytes<=8192);
  stream.update(camera('left'));
  await until(()=>stream.snapshot().entries.some(e=>e.index===0));
  assert.ok(requests.filter(n=>n===f.left.url).length>=2,'evicted leaf is reloaded');
  stream.dispose();

  // An uncooperative fetch may settle after abort. Its reservation and request
  // slot remain charged until then; stale bytes must never become resident.
  let release, calls=0;
  const delayed = new CadMetadataStream(f.index,{maxRequests:1,fetcher:async url=>{
    calls++;
    if (calls===1) await new Promise(resolve=>{release=resolve;});
    const name=new URL(url,location).pathname.split('/').at(-1);
    return new Response(f.assets.get(name));
  }});
  delayed.update(camera('left'));
  await until(()=>calls===1);
  delayed.update(camera('right'));
  assert.equal(calls,1,'cancelled fetch still occupies its request slot');
  assert.equal(delayed.snapshot().stats.reservedBytes,f.index.root.byte_length);
  release();
  await until(()=>delayed.snapshot().entries.some(e=>e.index===2));
  assert.deepEqual(delayed.snapshot().entries.map(e=>e.index).sort(),[2,3]);
  assert.equal(delayed.snapshot().stats.reservedBytes,0);
  delayed.dispose();

  const dup=fixture(true);
  const duplicate=new CadMetadataStream(dup.index,{fetcher:async url=>new Response(dup.assets.get(new URL(url,location).pathname.split('/').at(-1)))});
  duplicate.update({intersects:()=>true,priority:()=>1,refine:()=>true});
  await until(()=>duplicate.snapshot().stats.errors.length>0);
  assert.equal(duplicate.snapshot().entries.length,2,'conflicting leaf cannot retarget existing ordinal');
  assert.equal(duplicate.snapshot().proxies.length,1,'rejected subtree remains covered');
  duplicate.dispose();

  for (const mode of ['corrupt','oversized']) {
    const bad = new CadMetadataStream(f.index,{fetcher:async url=>{
      const name=new URL(url,location).pathname.split('/').at(-1);
      return new Response(name===f.left.url ? (mode==='corrupt'?'{}':'x'.repeat(1024*1024+1)) : f.assets.get(name));
    }});
    bad.update(camera('left'));
    await until(()=>Object.keys(bad.snapshot().stats.errors).length>0);
    assert.equal(bad.snapshot().entries.length,0);
    assert.ok(bad.snapshot().proxies.some(p=>p.url===f.left.url));
    assert.ok(bad.snapshot().stats.residentRecords===0);
    bad.dispose();
  }
  console.log(JSON.stringify({valid:true,checks:['lazy overview','selective traversal','record-budget eviction','reload identity','cancelled-request reservations','duplicate identity rejection','hash failure fallback','oversized response fallback']}));
}
run().catch(error=>{console.error(error);process.exitCode=1;});
