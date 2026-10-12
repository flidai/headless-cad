// Read-only inspection of nominal shaft-retention geometry and captured source.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'')+'/retained_shaft';
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}}),errors=[],writes=[];
  page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(r.method()!=='GET')writes.push(r.url());});
  const source=await(await page.request.get(`${base}/recipe.star`)).text();
  assert.ok((await page.goto(`${base}/retained_shaft.preview.html`)).ok());
  const derived=await page.evaluate(()=>{
   let count=0,index=0;
   for(const occurrence of compactScene.instances){
    const mesh=compactScene.meshes[compactScene.definitions[occurrence.part].mesh];
    for(const face of mesh.faces){
     const expanded=scene.faces[index++];
     face.edges.forEach((edge,i)=>{if(edge===mesh.edges){count++;if(creaseEdges.has(expanded.edges[i]))throw Error('Derived edge aliases another part boundary');}});
    }
   }
   return count;
  });assert.ok(derived>0);
  await page.locator('#recipe-toggle').click();
  const options=await page.locator('#occurrence-select option').evaluateAll(nodes=>nodes.map(n=>({value:n.value,text:n.textContent})));
  for(const name of ['shaft','left_bush','right_bush','washer','retaining_ring']){
   const option=options.find(o=>o.text.includes(name));assert.ok(option,`${name} occurrence missing`);
   await page.locator('#occurrence-select').selectOption(option.value);
   assert.equal(await page.locator('#recipe-source').textContent(),source);
   assert.match(await page.locator('#recipe-summary').textContent(),new RegExp(name));
  }
  const capture=async path=>{const cdp=await page.context().newCDPSession(page);const shot=await cdp.send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(path,Buffer.from(shot.data,'base64'));await cdp.detach();};
  await capture('/tmp/cad-retained-shaft.png');
  assert.ok((await page.goto(`${base}/retaining_ring.preview.html`)).ok());
  await capture('/tmp/cad-retaining-ring.png');
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);
  console.log(JSON.stringify({valid:true,parts:['shaft','left_bush','right_bush','washer','retaining_ring'],captured_source:true,read_only:true,errors},null,2));
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exit(1)});
