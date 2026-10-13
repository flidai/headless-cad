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
    viewBox:root.getAttribute('viewBox'),title:root.querySelector('title')?.textContent,
    tables:[...root.querySelectorAll('g[id^="drawing-table-"]')].filter(g=>!g.hasAttribute('data-dimension')).map(g=>({
      id:g.id,bounds:rect(g),rows:[...g.querySelectorAll('g[data-dimension]')].map(row=>({
       id:row.id,dimension:row.dataset.dimension,text:' '.concat(row.textContent).replace(/\s+/g,' ').trim(),
       bounds:rect(row)}))})),
    footer:[...root.querySelectorAll('text')].map(t=>t.textContent).filter(Boolean)};
  });
  assert.equal(visual.width,'420mm');assert.equal(visual.height,'297mm');
  assert.equal(visual.viewBox,'0 0 420 297');
  assert.equal(visual.tables.length,2);
  assert.deepEqual(new Set(sheet.dimension_tables.map(t=>t.id)),new Set(['left_pilots','right_pilots']));
  const seen=new Set();
  for(const table of sheet.dimension_tables){
   const rendered=visual.tables.find(t=>t.id===`drawing-table-${table.id}`);
   assert.ok(rendered,`visible ${table.id}`);assert.equal(table.output,'drive_module_release');
   assert.equal(table.rows.length,5);assert.equal(rendered.rows.length,5);
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
    const [rx,ry,rw,rh]=actual.bounds;
    assert.ok(rx>=box.x-.02&&ry>=box.y-.02&&rx+rw<=box.x+box.width+.02&&ry+rh<=box.y+box.height+.02,
      `row ${id} overflows paper box`);
   }
  }
  assert.equal(seen.size,10);
  assert.ok(visual.footer.some(t=>t.includes('Nominal native geometry')));
  assert.ok(visual.footer.some(t=>t.includes(`Revision: ${sheet.declaration.revision}`)));
  assert.ok(visual.footer.some(t=>t.includes(`Source context: ${manifest.recipe_context_hash}`)));
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  const cdp=await page.context().newCDPSession(page);
  const image=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});
  require('node:fs').writeFileSync(process.env.CAD_SCREENSHOT||'/tmp/cad-end-cover-machining-drawing.png',Buffer.from(image.data,'base64'));
  await cdp.detach();
  console.log(JSON.stringify({valid:true,rows:[...seen],source_context_hash:manifest.recipe_context_hash,
   tables:visual.tables.map(({id,bounds})=>({id,bounds})),read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
