// Browser acceptance of source-bound native dimension tables; display is read-only.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/drive_module_rotation';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1600,height:1180},deviceScaleFactor:1}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));
  page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  const [manifest,sheet,datum]=await Promise.all([
   page.request.get(`${base}/manifest.json`).then(r=>r.json()),
   page.request.get(`${base}/drawing-end_cover_machining.json`).then(r=>r.json()),
   page.request.get(`${base}/datum-dimensions.json`).then(r=>r.json()),
  ]);
  assert.equal(sheet.schema_version,2);assert.equal(sheet.status,'published');
  assert.equal(sheet.recipe_context_hash,manifest.recipe_context_hash);
  assert.deepEqual(sheet.recipe_source,manifest.recipe_source);
  assert.equal(sheet.datum_dimension_report.file,'datum-dimensions.json');
  assert.equal(sheet.datum_dimension_report.digest,manifest.artifact_integrity.files['datum-dimensions.json'].digest);
  assert.equal(datum.recipe_context_hash,manifest.recipe_context_hash);
  const response=await page.goto(`${base}/drawing-end_cover_machining.svg`);assert.ok(response.ok());
  const visual=await page.evaluate(()=>{
   const root=document.documentElement;
   const rect=element=>{const b=element.getBBox();return [b.x,b.y,b.width,b.height]};
   return {width:root.getAttribute('width'),height:root.getAttribute('height'),
    paperScreen:(()=>{const b=root.getBoundingClientRect();return [b.left,b.top,b.width,b.height]})(),
    viewBox:root.getAttribute('viewBox'),title:root.querySelector('title')?.textContent,
    views:[...root.querySelectorAll('svg[id^="drawing-view-"]')].map(view=>{
     const vr=view.getBoundingClientRect();
     const caption=root.querySelector(`#drawing-caption-${view.id.slice('drawing-view-'.length)}`);
     return {id:view.id,scale:Number(view.dataset.scale),paperFont:Number(view.dataset.paperAnnotationFontMm),
      renderedBounds:rect(view),viewBox:view.getAttribute('viewBox'),
      viewport:[vr.left,vr.top,vr.right,vr.bottom],
      caption:caption&&{text:caption.textContent,font:Number(caption.getAttribute('font-size')),
       box:(()=>{const b=caption.getBoundingClientRect();return [b.left,b.top,b.right,b.bottom]})()},
      annotations:[...view.querySelectorAll('g.section-circle-dimension,g.section-dimension')].map(group=>{
       const text=group.querySelector('text'),b=text.getBoundingClientRect();
       return {id:group.dataset.id,value:Number(group.dataset.valueMm),kind:group.dataset.kind||null,
        text:text.textContent,font:Number(text.getAttribute('font-size')),
        hasTextLength:text.hasAttribute('textLength'),family:text.getAttribute('font-family'),
        box:[b.left,b.top,b.right,b.bottom]};
      })};
    }),
    tables:[...root.querySelectorAll('g[id^="drawing-table-"]')].filter(g=>!g.hasAttribute('data-dimension')).map(g=>({
      id:g.id,bounds:rect(g),rows:[...g.querySelectorAll('g[data-dimension]')].map(row=>({
       id:row.id,dimension:row.dataset.dimension,text:' '.concat(row.textContent).replace(/\s+/g,' ').trim(),
       bounds:rect(row)}))})),
    footer:[...root.querySelectorAll('text')].map(t=>t.textContent).filter(Boolean)};
  });
  assert.equal(visual.width,'420mm');assert.equal(visual.height,'297mm');
  assert.equal(visual.viewBox,'0 0 420 297');
  assert.equal(visual.tables.length,2);
  assert.equal(visual.views.length,3);
  const nativeAnnotations=new Set();
  for(const evidence of sheet.views){
   const source=evidence.viewport,view=visual.views.find(v=>v.id===`drawing-view-${source.view}`);
   assert.ok(view,`published ${source.view}`);
   assert.equal(view.scale,source.scale);assert.equal(view.paperFont,2.8);
   assert.equal(source.paper_annotation_font_mm,2.8);
   assert.equal(view.caption?.text,source.paper_caption);assert.equal(view.caption?.font,2.8);
   const [paperLeft,paperTop,paperWidth]=visual.paperScreen,pxPerMm=paperWidth/420;
   const [cl,ct,cr,cb]=view.caption.box;
   assert.ok(cl>=paperLeft+source.x*pxPerMm-2&&cr<=paperLeft+(source.x+source.width)*pxPerMm+2&&
             ct>=paperTop+source.y*pxPerMm-2&&cb<=view.viewport[1]+2,
             `paper caption fits its own band in ${source.view}`);
   assert.ok([.5,.8].includes(view.scale),`supported print-scale proof ${source.view}`);
   const section=await(await page.request.get(`${base}/${evidence.section_report}`)).json();
   const reports=new Map(section.section_view.circle_dimensions.map(r=>[r.declaration.id,r]));
   assert.equal(view.annotations.length,source.annotation_labels.length);
   const tags=new Map(source.annotation_labels.map(a=>[a.dimension,a.tag]));
   for(const annotation of view.annotations){
    const native=reports.get(annotation.id),tag=tags.get(annotation.id);
    assert.ok(native&&tag,`native source mapping ${annotation.id}`);
    assert.equal(native.status,'passed');assert.equal(annotation.kind,'diameter');
    assert.ok(Math.abs(annotation.value-native.value_mm)<1e-8);
    assert.ok(Math.abs(annotation.font*view.scale-2.8)<1e-9,
     `printed text ${annotation.id} remains 2.8 mm at scale ${view.scale}`);
    assert.equal(annotation.family,'monospace');assert.equal(annotation.hasTextLength,false);
    const printed=annotation.text.match(/^([A-Za-z][A-Za-z0-9]*):⌀([0-9.eE+-]+)mm$/);
    assert.ok(printed&&printed[1]===tag,`visible native diameter tag ${annotation.id}: ${annotation.text}`);
    const measured=Number(printed[2]);
    assert.ok(Math.abs(measured-native.value_mm)<1e-5,`native number ${annotation.id}`);
    const [l,t,r,b]=annotation.box,[vl,vt,vr,vb]=view.viewport;
    assert.ok(l>=vl-2&&t>=vt-2&&r<=vr+2&&b<=vb+2,`annotation ${annotation.id} fits view`);
    nativeAnnotations.add(annotation.id);
   }
   for(let i=0;i<view.annotations.length;i++)for(const other of view.annotations.slice(i+1)){
    const a=view.annotations[i].box,b=other.box;
    const overlap=Math.max(0,Math.min(a[2],b[2])-Math.max(a[0],b[0]))*
                  Math.max(0,Math.min(a[3],b[3])-Math.max(a[1],b[1]));
    assert.ok(overlap<.5,`visible annotation texts overlap in ${source.view}`);
   }
   const standalone=await(await page.request.get(`${base}/section-${source.view}.svg`)).text();
   for(const {dimension,tag} of source.annotation_labels){
    assert.ok(standalone.includes(`data-id="${dimension}"`),`standalone native ID ${dimension}`);
    assert.ok(!standalone.includes(`${tag}:⌀`),`standalone section uses full ID ${dimension}`);
   }
  }
  assert.equal(nativeAnnotations.size,7);
  assert.deepEqual(new Set(sheet.dimension_tables.map(t=>t.id)),new Set(['left_pilots','right_pilots']));
  const seen=new Set();
  for(const table of sheet.dimension_tables){
   const rendered=visual.tables.find(t=>t.id===`drawing-table-${table.id}`);
   assert.ok(rendered,`visible ${table.id}`);assert.equal(table.output,'drive_module_release');
   assert.equal(table.show_size_limits,true);
   assert.equal(table.rows.length,7);assert.equal(rendered.rows.length,7);
   const box=table.paper_box_mm,[x,y,w,h]=rendered.bounds;
   assert.ok(x>=box.x-.02&&y>=box.y-.02&&x+w<=box.x+box.width+.02&&y+h<=box.y+box.height+.02,
    `table ${table.id} overflows paper box`);
   for(const row of table.rows){
    const id=row.dimension,actual=rendered.rows.find(r=>r.dimension===id),native=datum.dimensions[id];
    assert.ok(actual&&native,`visible current native row ${id}`);assert.ok(!seen.has(id));seen.add(id);
    assert.equal(actual.id,`drawing-table-${table.id}-${id}`);
    assert.deepEqual(row.native_dimension,native);
    assert.deepEqual(native,manifest.datum_dimensions[id]);
    assert.equal(native.status,'passed');assert.equal(native.declaration.output,table.output);
    assert.ok(native.lower_bound_mm<=native.value_mm&&native.value_mm<=native.upper_bound_mm);
    for(const end of [native.from,native.to]){
     assert.equal(end.semantic_datum.selection.status,'resolved');
     assert.equal(end.semantic_datum.selection.witnesses.length,1);
     assert.equal(end.semantic_datum.target_content_hash,end.part_content_hash);
    }
    assert.ok(actual.text.includes(`≈ ${native.value_mm.toFixed(3)} mm`),`${id}: ${actual.text}`);
    assert.ok(actual.text.includes(row.label),`source-authored label ${id}`);
    if(native.tolerance){
     const limits=[native.tolerance.lower_limit_mm,native.tolerance.upper_limit_mm];
     assert.deepEqual(row.authored_size_limits_mm,limits);
     const shown=actual.text.match(/([-+0-9.eE]+)\.\.([-+0-9.eE]+) mm/);
     assert.ok(shown,`explicit authored limits ${id}`);
     assert.deepEqual(shown.slice(1).map(Number),limits,`exact authored limits ${id}`);
    }else{
     assert.equal(row.authored_size_limits_mm,null);
     assert.ok(actual.text.includes('not specified'),`no implied limits ${id}`);
    }
    const [rx,ry,rw,rh]=actual.bounds;
    assert.ok(rx>=box.x-.02&&ry>=box.y-.02&&rx+rw<=box.x+box.width+.02&&ry+rh<=box.y+box.height+.02,
      `row ${id} overflows paper box`);
   }
  }
  assert.equal(seen.size,14);
  assert.ok(visual.footer.some(t=>t.includes('size limits authored, or not specified')));
  assert.ok(visual.footer.some(t=>t.includes(`Revision: ${sheet.declaration.revision}`)));
  assert.ok(visual.footer.some(t=>t.includes(`Source context: ${manifest.recipe_context_hash}`)));
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  const cdp=await page.context().newCDPSession(page);
  const image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  require('node:fs').writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-end-cover-machining-drawing.png',Buffer.from(image.data,'base64'));
  await cdp.detach();
  let halfScale=null;
  if(process.env.CAD_HALF_SCALE_SVG){
   const {pathToFileURL}=require('node:url');
   assert.ok((await page.goto(pathToFileURL(process.env.CAD_HALF_SCALE_SVG).href)).ok());
   halfScale=await page.evaluate(()=>{
    const view=document.querySelector('#drawing-view-left_cover_pilot_section');
    return {scale:Number(view.dataset.scale),paperFont:Number(view.dataset.paperAnnotationFontMm),
     captionFont:Number(document.querySelector('#drawing-caption-left_cover_pilot_section').getAttribute('font-size')),
     labels:[...view.querySelectorAll('g.section-circle-dimension text')].map(t=>({
      text:t.textContent,modelFont:Number(t.getAttribute('font-size')),
      hasTextLength:t.hasAttribute('textLength')}))};
   });
   assert.equal(halfScale.scale,.5);assert.equal(halfScale.paperFont,2.8);
   assert.equal(halfScale.captionFont,2.8);assert.equal(halfScale.labels.length,2);
   for(const label of halfScale.labels){
    assert.ok(Math.abs(label.modelFont*halfScale.scale-2.8)<1e-9);
    assert.equal(label.hasTextLength,false);
   }
  }
  console.log(JSON.stringify({valid:true,rows:[...seen],source_context_hash:manifest.recipe_context_hash,
   tables:visual.tables.map(({id,bounds})=>({id,bounds})),
   paper_annotations:{count:nativeAnnotations.size,scales:visual.views.map(v=>v.scale),half_scale:halfScale},
   read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
