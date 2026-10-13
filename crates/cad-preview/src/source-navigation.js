// Exact captured-source navigation. No parsing heuristics, requests, or modeling calls.
window.CadSourceNavigation=class {
 constructor({picker,source,row=null}){
  this.picker=picker;this.source=source;this.row=row;this.key=null;this.context=null;
  this.navigation=document.createElement('nav');this.navigation.id='recipe-declarations';this.navigation.hidden=true;this.navigation.setAttribute('aria-label','Captured declaration locations');
  this.location=document.createElement('p');this.location.id='recipe-source-location';this.location.className='muted';this.location.setAttribute('aria-live','polite');
  source.before(this.navigation,this.location);
  const style=document.createElement('style');style.textContent='#recipe-declarations{display:flex;gap:6px;flex-wrap:wrap;margin:12px 0}#recipe-declarations[hidden]{display:none}#recipe-declarations button{font:inherit;font-size:12px;padding:6px 8px;border:1px solid #a3b8aa;border-radius:5px;background:#fff;color:inherit;cursor:pointer}#recipe-declarations button[aria-pressed="true"]{background:#dbeee1;border-color:#477c60}#recipe-source-highlight{background:#f7df99;color:#26332c;border-radius:2px}#recipe-source-location{overflow-wrap:anywhere}';document.head.append(style);
 }
 async render(context,selection=null){
  const previous=this.picker.value,changed=this.context!==context;
  this.context=context;
  const modules=context?.modules||{},labels=Object.keys(modules).sort();
  if(changed){
   this.verification=null;
   this.picker.replaceChildren();
   for(const [value,label] of [['',context?.source_name||'Root recipe'],...labels.map(x=>[x,x])]){const option=document.createElement('option');option.value=value;option.textContent=label;this.picker.append(option)}
   this.picker.value=labels.includes(previous)?previous:'';
   if(this.row)this.row.hidden=labels.length===0;
   this.picker.onchange=()=>{this.manualKey=this.key;this.whole();};
  }
  const key=JSON.stringify([context?.recipe_context_hash,selection?.instance_id,selection?.part_id,selection?.feature_id||context?.feature_id]);
  if(!changed&&key===this.key)return;
  this.key=key;this.navigation.replaceChildren();this.navigation.hidden=true;
  this.whole();
  const report=context?.source_provenance;
  if(!report)return;
  try{
   if(context.language!=='starlark'||report.schema_version!==1||report.kind!=='source_provenance'||report.recipe_context_hash!==context.recipe_context_hash||(report.source_bundle_hash??null)!==(context.source_bundle_hash??null))throw Error('Declaration evidence does not match this captured source');
   const inventory=report.source_inventory;
   if(!inventory||Object.keys(inventory).length!==labels.length+1)throw Error('Incomplete captured source inventory');
   for(const [name,text] of [[context.source_name,context.source],...Object.entries(modules)])if(typeof text!=='string'||inventory[name]?.bytes!==new TextEncoder().encode(text).length)throw Error('Captured source inventory mismatch');
   if(changed||!this.verification){
    if(!window.crypto?.subtle)throw Error('Source verification needs HTTPS, localhost, or a local file. Whole captured source remains available.');
    this.verification=Promise.all([[context.source_name,context.source],...Object.entries(modules)].map(async([name,text])=>{
     const expected=inventory[name]?.sha256;
     if(typeof expected!=='string'||!/^[0-9a-f]{64}$/.test(expected))throw Error('Missing captured source digest');
     const digest=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(text))),v=>v.toString(16).padStart(2,'0')).join('');
     if(digest!==expected)throw Error('Captured source digest mismatch');
    }));
   }
   await this.verification;
   if(this.key!==key||this.context!==context)return;
   const part=selection?.part_id,feature=report.part_body_features?.[part]||selection?.feature_id||context.feature_id;
   const entries=[];
   if(part&&report.parts?.[part])entries.push(['part','Part declaration',report.parts[part]]);
   if(feature&&report.features?.[feature])entries.push(['feature','Body feature',report.features[feature]]);
   if(!entries.length)return;
   const button=(role,label,span,after=null)=>{
    // Validate before enabling navigation, including UTF-8 boundaries and line/column metadata.
    this.decode(span);
    const b=document.createElement('button');b.type='button';b.dataset.sourceRole=role;b.textContent=label;b.setAttribute('aria-pressed','false');
    if(role==='part'||role==='feature')b.id=`recipe-${role}-source`;
    b.onclick=()=>{try{this.show(span);for(const other of this.navigation.querySelectorAll('button'))other.setAttribute('aria-pressed',String(other===b));after?.();}catch(error){this.navigation.hidden=true;this.whole();this.location.textContent=error.message;}};
    this.navigation.append(b);return b;
   };
   const callers=entry=>{
    for(const existing of this.navigation.querySelectorAll('[data-source-role="caller"]'))existing.remove();
    const calls=entry.call_stack;
    if(!Array.isArray(calls)||calls.length>64)throw Error('Invalid declaration call stack');
    const seen=new Set([JSON.stringify(entry.declaration)]);
    for(const call of calls){const token=JSON.stringify(call);if(seen.has(token))continue;seen.add(token);button('caller',`Called from ${call.file}:${call.start_line}`,call)}
   };
   const buttons=entries.map(([role,label,entry])=>button(role,label,entry.declaration,()=>callers(entry)));
   this.navigation.hidden=false;if(this.manualKey!==key)buttons[0].click();
  }catch(error){if(this.key!==key||this.context!==context)return;this.navigation.replaceChildren();this.navigation.hidden=true;this.whole();this.location.textContent=error.message;}
 }
 text(file){if(file===this.context?.source_name)return this.context.source;if(Object.hasOwn(this.context?.modules||{},file))return this.context.modules[file];throw Error('Declaration is outside the captured source bundle');}
 decode(span){
  if(!span||typeof span.file!=='string'||!Number.isSafeInteger(span.start_byte)||!Number.isSafeInteger(span.end_byte)||span.start_byte<0||span.end_byte<=span.start_byte)throw Error('Invalid declaration byte range');
  const text=this.text(span.file),bytes=new TextEncoder().encode(text);
  if(span.end_byte>bytes.length)throw Error('Declaration extends beyond captured source');
  const decode=bytes=>new TextDecoder('utf-8',{fatal:true,ignoreBOM:true}).decode(bytes);
  const before=decode(bytes.subarray(0,span.start_byte)),body=decode(bytes.subarray(span.start_byte,span.end_byte)),after=decode(bytes.subarray(span.end_byte));
  const position=s=>{const lines=s.split('\n');return[lines.length,Array.from(lines.at(-1)).length+1]};
  const start=position(before),end=position(before+body);
  if(start[0]!==span.start_line||start[1]!==span.start_column||end[0]!==span.end_line||end[1]!==span.end_column)throw Error('Declaration coordinates do not match captured UTF-8 source');
  return{before,body,after};
 }
 show(span){
  const {before,body,after}=this.decode(span);
  this.picker.value=span.file===this.context.source_name?'':span.file;
  const mark=document.createElement('mark');mark.id='recipe-source-highlight';mark.textContent=body;
  this.source.replaceChildren(document.createTextNode(before),mark,document.createTextNode(after));
  this.location.textContent=`${span.file}:${span.start_line}:${span.start_column} · captured declaration`;
  this.source.dataset.sourceFile=span.file;this.source.dataset.startByte=String(span.start_byte);this.source.dataset.endByte=String(span.end_byte);
  requestAnimationFrame(()=>{if(mark.isConnected)this.source.scrollTop+=mark.getBoundingClientRect().top-this.source.getBoundingClientRect().top-28;});
 }
 whole(){
  this.source.textContent=this.picker.value?this.context?.modules?.[this.picker.value]||'Source unavailable':this.context?.source||'No recipe source embedded.';
  this.location.textContent='';delete this.source.dataset.sourceFile;delete this.source.dataset.startByte;delete this.source.dataset.endByte;
  for(const b of this.navigation.querySelectorAll('button'))b.setAttribute('aria-pressed','false');
 }
};
