// Verify source-bound read-only inspection of both real blind-hole floors.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');

const base=(process.env.CAD_BASE_URL||'http://localhost:8080').replace(/\/$/,'');
const fixtures=[
 {path:process.env.CAD_HOUSING_TOP_PATH||'sketched_blind_housing_top',side:'top',openZ:12},
 {path:process.env.CAD_HOUSING_BOTTOM_PATH||'sketched_blind_housing_bottom',side:'bottom',openZ:0},
];

(async()=>{
 const browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,
  args:['--no-sandbox']});
 try{
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  const errors=[],writes=[],exact=[];
  page.on('pageerror',error=>errors.push(error.message));
  page.on('request',request=>{
   if(request.method()!=='GET')writes.push(`${request.method()} ${request.url()}`);
   if(/\.(brep\.json|step|stl)(\?|$)/.test(request.url()))exact.push(request.url());
  });
  const results=[];
  for(const fixture of fixtures){
   const url=`${base}/${fixture.path}`;
   const manifest=await(await page.request.get(`${url}/manifest.json`)).json();
   const section=await(await page.request.get(`${url}/section-clamp_holes.json`)).json();
   const dimensionIds=new Set(section.section_view.circle_dimensions.map(d=>d.declaration.id));
   assert.deepEqual(dimensionIds,new Set(['left_clamp_diameter','right_clamp_diameter']));
   const svg=await(await page.request.get(`${url}/section-clamp_holes.svg`)).text();
   for(const id of dimensionIds)assert.ok(svg.includes(`data-id="${id}"`),id);
   const source=await(await page.request.get(`${url}/recipe.star`)).text();
   assert.match(source,/hole_pattern\(/);
   assert.ok((await page.goto(`${url}/housing.preview.html`)).ok());
   const floorIds=['clamp_left_floor','clamp_right_floor'].map(name=>{
    const report=manifest.face_assertions[name];
    assert.equal(report.status,'resolved');
    return report.witnesses[0].target_face;
   });
   const picked=await page.evaluate(({side,openZ})=>{
    yaw=-Math.PI/2;pitch=side==='top'?Math.PI/2:-Math.PI/2;draw();
    const right=[-Math.sin(yaw),Math.cos(yaw),0];
    const up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
    const dot=(a,b)=>a.reduce((s,v,i)=>s+v*b[i],0);
    const plane=q=>{q=q.map((v,i)=>v-center[i]);return[dot(q,right),-dot(q,up)];};
    const extent=[0,0];
    for(const face of scene.faces)for(const vertex of face.points){
     const q=plane(vertex);extent[0]=Math.max(extent[0],Math.abs(q[0]));extent[1]=Math.max(extent[1],Math.abs(q[1]));
    }
    const scale=zoom*Math.min((width-80)/(2*extent[0]||1),(height-150)/(2*extent[1]||1));
    return[-16,16].map(x=>{
     const q=plane([x,0,openZ]);
     const px=width/2+q[0]*scale,py=height/2+20+q[1]*scale;
     const index=faceBuffer[Math.floor(py*renderRatio)*bufferWidth+Math.floor(px*renderRatio)];
     return index>=0?scene.faces[index].semantic_face_id:-1;
    });
   },{side:fixture.side,openZ:fixture.openZ});
   assert.deepEqual(picked,floorIds,`${fixture.side} actual blind floors`);
   await page.locator('#recipe-toggle').click();
   assert.equal(await page.locator('#recipe-source').textContent(),source);
   assert.equal(await page.locator('#recipe-source').isVisible(),true);
   if(fixture.side==='top')await page.screenshot({path:'/tmp/cad-sketched-blind-housing.png'});
   results.push({side:fixture.side,floor_faces:picked,captured_source:true,
    diameter_labels:[...dimensionIds]});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(writes,[]);assert.deepEqual(exact,[]);
  console.log(JSON.stringify({valid:true,results,read_only:true,exact_geometry_downloads:0},null,2));
 }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exit(1)});
