/* Verified, bounded metadata residency. Encoded budgets are not JS heap limits. */
(function (scope) {
'use strict';
const PAGE_LIMIT=1048576, PAGE_COUNT=256;
const integer=n=>Number.isSafeInteger(n)&&n>=0;
const object=v=>v!==null&&typeof v==='object'&&!Array.isArray(v);
const vector=(v,n)=>Array.isArray(v)&&v.length===n&&v.every(Number.isFinite);
const bounds=b=>Array.isArray(b)&&b.length===2&&b.every(v=>vector(v,3))&&b[0].every((v,i)=>v<=b[1][i]);
const contains=(a,b)=>b[0].every((v,i)=>v>=a[0][i]&&b[1][i]<=a[1][i]);
const check=(condition,message)=>{if(!condition)throw new Error(message);};
function descriptor(d,mesh=false){
 check(object(d)&&/^[0-9a-f]{64}$/.test(d.sha256),'invalid digest');
 check(d.url===`${mesh?'mesh':'metadata'}-${d.sha256}.display.json`,'unsafe page URL');
 check(integer(d.byte_length)&&d.byte_length>0&&(mesh||d.byte_length<=PAGE_LIMIT),'invalid byte length');
 check(mesh?(d.bounds===null||bounds(d.bounds)):bounds(d.bounds),'invalid bounds');
 if(!mesh)check(integer(d.occurrences)&&d.occurrences>0,'invalid occurrence count');
}
function transform(t){check(object(t)&&vector(t.translation,3)&&vector(t.rotation,4),'invalid transform');check(Math.abs(Math.hypot(...t.rotation)-1)<1e-10,'nonunit rotation');}
class CadMetadataStream {
 constructor(index,{budgetBytes=8*1048576,maxRecords=4096,maxRequests=4,onChange=()=>{},fetcher=scope.fetch?.bind(scope)}={}){
  check(object(index)&&index.schema_version===1&&index.kind==='assembly_metadata_index'&&integer(index.total_occurrences),'invalid metadata index');
  check(typeof index.assembly_id==='string'&&typeof index.content_hash==='string','missing index identity');
  if(index.root!==null){descriptor(index.root);check(index.root.occurrences===index.total_occurrences,'root count mismatch');}else check(index.total_occurrences===0,'missing root');
  check(integer(budgetBytes)&&budgetBytes>0&&integer(maxRecords)&&maxRecords>0&&integer(maxRequests)&&maxRequests>0&&maxRequests<=32,'invalid scheduler limits');
  check(typeof fetcher==='function','fetch unavailable');
  this.index=JSON.parse(JSON.stringify(index));this.budgetBytes=budgetBytes;this.maxRecords=maxRecords;this.maxRequests=maxRequests;this.onChange=onChange;this.fetcher=fetcher;
  this.pages=new Map();this.pending=new Map();this.failed=new Set();this.epoch=0;this.clock=0;this.dead=false;
  this.stats={residentBytes:0,reservedBytes:0,residentRecords:0,residentPages:0,requested:0,aborted:0,errors:[]};
  this.view={intersects:()=>true,priority:()=>0,refine:()=>true};
 }
 update(view){if(this.dead)return;for(const k of ['intersects','priority','refine'])check(typeof view[k]==='function',`missing ${k}`);this.view=view;this.epoch++;this.failed.clear();for(const p of this.pending.values()){if(!p.controller.signal.aborted){p.controller.abort();this.stats.aborted++;}}this.pump();}
 walk(){
  const entries=[],proxies=[],needed=new Map(),demand=new Set(),seen=new Set();
  const visit=(d,ancestors)=>{
   if(!this.view.intersects(d.bounds))return;
   demand.add(d.url);
   if(!this.view.refine(d.bounds)){proxies.push(d);return;}
   const loaded=this.pages.get(d.url);
   if(!loaded){proxies.push(d);if(!this.failed.has(d.url))needed.set(d.url,{descriptor:d,ancestors});return;}
   loaded.used=++this.clock;
   if(loaded.page.kind==='assembly_metadata_leaf'){
    for(const e of loaded.page.entries){if(seen.has(e.index)){proxies.push(d);return;}seen.add(e.index);}entries.push(...loaded.page.entries);
   }else for(const child of loaded.page.children)visit(child,[...ancestors,d.url]);
  };
  if(this.index.root)visit(this.index.root,[]);
  return {entries,proxies,needed,demand};
 }
 snapshot(){const {entries,proxies}=this.walk();return {entries,proxies,stats:{...this.stats,pendingRequests:this.pending.size,errors:[...this.stats.errors]}};}
 evict(url){const p=this.pages.get(url);if(!p)return;this.pages.delete(url);this.stats.residentBytes-=p.bytes;this.stats.residentRecords-=p.records;this.stats.residentPages=this.pages.size;}
 room(bytes,records,demand){
  const fits=()=>this.stats.residentBytes+this.stats.reservedBytes+bytes<=this.budgetBytes&&this.stats.residentRecords+records<=this.maxRecords&&this.pages.size+this.pending.size<PAGE_COUNT;
  const victims=[...this.pages.entries()].filter(([url])=>!demand.has(url)).sort((a,b)=>a[1].used-b[1].used||a[0].localeCompare(b[0]));
  while(!fits()&&victims.length)this.evict(victims.shift()[0]);return fits();
 }
 pump(){
  if(this.dead)return;
  const state=this.walk();
  const requests=[...state.needed.values()].sort((a,b)=>{
   const priority=d=>{const v=this.view.priority(d.bounds);return Number.isFinite(v)?v:0;};
   return priority(b.descriptor)-priority(a.descriptor)||a.descriptor.url.localeCompare(b.descriptor.url);
  });
  for(const item of requests){
   const d=item.descriptor;if(this.pending.size>=this.maxRequests)break;if(this.pending.has(d.url))continue;
   if(!this.room(d.byte_length,0,state.demand))continue;
   const request={controller:new AbortController(),epoch:this.epoch,descriptor:d};
   this.pending.set(d.url,request);this.stats.reservedBytes+=d.byte_length;this.stats.requested++;
   this.load(d,request,item.ancestors);
  }
 }
 validatePage(page,d,ancestors){
  check(object(page)&&page.schema_version===1,'invalid page schema');
  let records=0;
  if(page.kind==='assembly_metadata_branch'){
   check(Array.isArray(page.children)&&page.children.length===2,'invalid branch');
   let count=0;const urls=new Set();
   for(const child of page.children){descriptor(child);check(contains(d.bounds,child.bounds),'child outside parent');check(!urls.has(child.url)&&child.url!==d.url&&!ancestors.includes(child.url),'duplicate/cyclic child');urls.add(child.url);count+=child.occurrences;}
   check(Number.isSafeInteger(count)&&count===d.occurrences,'branch count mismatch');
  }else{
   check(page.kind==='assembly_metadata_leaf'&&Array.isArray(page.entries)&&page.entries.length>0&&page.entries.length<=128&&page.entries.length===d.occurrences,'invalid leaf count');
   const indices=new Set(),resident=new Set();for(const value of this.pages.values())if(value.page.entries)for(const e of value.page.entries)resident.add(e.index);
   for(const e of page.entries){
    check(object(e)&&integer(e.index)&&e.index<this.index.total_occurrences&&!indices.has(e.index)&&!resident.has(e.index),'duplicate/out-of-range ordinal');indices.add(e.index);
    check(typeof e.empty==='boolean'&&object(e.instance)&&typeof e.instance.id==='string'&&e.instance.id.length>0&&typeof e.instance.part==='string','invalid instance');transform(e.instance.transform);
    check(Array.isArray(e.path)&&e.path.length>0&&e.path.length<=64&&e.path.every(p=>typeof p==='string'&&p.length>0&&p.length<=1024),'invalid path');
    const f=e.definition;check(object(f)&&typeof f.mesh==='string'&&typeof f.feature_id==='string'&&object(f.metadata)&&object(f.datums),'invalid definition');for(const t of Object.values(f.datums))transform(t);
    descriptor(f.mesh_asset,true);check(e.empty===(f.mesh_asset.bounds===null),'empty bounds mismatch');check(Array.isArray(f.mesh_lods)&&f.mesh_lods.length>0&&f.mesh_lods.length<=3,'invalid LODs');for(const lod of f.mesh_lods)descriptor(lod,true);
    check(f.mesh_lods.at(-1).sha256===f.mesh_asset.sha256,'fine LOD mismatch');
    const local=f.mesh_asset.bounds||[[0,0,0],[0,0,0]],t=e.instance.transform;
    for(let mask=0;mask<8;mask++){
     const p=[0,1,2].map(i=>local[(mask>>i)&1][i]),[x,y,z,w]=t.rotation;
     const cross=[2*(y*p[2]-z*p[1]),2*(z*p[0]-x*p[2]),2*(x*p[1]-y*p[0])];
     const rotated=[p[0]+w*cross[0]+y*cross[2]-z*cross[1],p[1]+w*cross[1]+z*cross[0]-x*cross[2],p[2]+w*cross[2]+x*cross[1]-y*cross[0]].map((v,i)=>v+t.translation[i]);
     check(rotated.every((v,i)=>Number.isFinite(v)&&v>=d.bounds[0][i]&&v<=d.bounds[1][i]),'leaf display bounds outside page');
    }
   }
   records=page.entries.length;
  }
  return records;
 }
 async load(d,request,ancestors){
  try{
   const response=await this.fetcher(d.url,{signal:request.controller.signal,redirect:'error',credentials:'same-origin'});
   if(this.dead||request.controller.signal.aborted||request.epoch!==this.epoch){await response.body?.cancel().catch(()=>{});return;}
   check(response.ok,'metadata request failed');
   const header=response.headers?.get('content-length');if(header!==null&&header!==undefined)check(Number(header)===d.byte_length,'content length mismatch');
   check(response.body&&typeof response.body.getReader==='function','bounded streaming response required');
   const reader=response.body.getReader(),chunks=[];let length=0;
   const cancel=()=>{reader.cancel().catch(()=>{});};request.controller.signal.addEventListener('abort',cancel,{once:true});
   try{for(;;){const {done,value}=await reader.read();if(request.controller.signal.aborted||this.dead)return;if(done)break;length+=value.byteLength;check(length<=d.byte_length&&length<=PAGE_LIMIT,'metadata byte budget exceeded');chunks.push(value);}}
   catch(error){await reader.cancel().catch(()=>{});throw error;}finally{request.controller.signal.removeEventListener('abort',cancel);reader.releaseLock();}
   if(this.dead||request.controller.signal.aborted||request.epoch!==this.epoch)return;
   check(length===d.byte_length,'truncated metadata page');
   const bytes=new Uint8Array(length);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.byteLength;}
   const hash=Array.from(new Uint8Array(await scope.crypto.subtle.digest('SHA-256',bytes)),v=>v.toString(16).padStart(2,'0')).join('');check(hash===d.sha256,'metadata digest mismatch');
   if(this.dead||request.epoch!==this.epoch||this.pending.get(d.url)!==request)return;
   const page=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));const records=this.validatePage(page,d,ancestors);
   this.pending.delete(d.url);this.stats.reservedBytes-=d.byte_length;
   const state=this.walk();
   if(!this.room(d.byte_length,records,state.demand)){this.failed.add(d.url);return;}
   this.pages.set(d.url,{page,records,bytes:d.byte_length,used:++this.clock});this.stats.residentBytes+=d.byte_length;this.stats.residentRecords+=records;this.stats.residentPages=this.pages.size;
  }catch(error){
   if(!this.dead&&request.epoch===this.epoch&&error.name!=='AbortError'){this.failed.add(d.url);this.stats.errors.push(String(error.message||error));if(this.stats.errors.length>32)this.stats.errors.shift();}
  }finally{
   if(this.pending.get(d.url)===request){this.pending.delete(d.url);this.stats.reservedBytes-=d.byte_length;}
   if(!this.dead){this.pump();this.onChange();}
  }
 }
 dispose(){this.dead=true;for(const p of this.pending.values()){if(!p.controller.signal.aborted){p.controller.abort();this.stats.aborted++;}}this.pages.clear();Object.assign(this.stats,{residentBytes:0,residentRecords:0,residentPages:0});}
}
scope.CadMetadataStream=CadMetadataStream;
if(typeof module!=='undefined'&&module.exports)module.exports={CadMetadataStream};
})(globalThis);
