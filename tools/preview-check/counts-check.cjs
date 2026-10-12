// Native topology counts must remain distinct from derived display patches,
// including repeated occurrences, legacy payloads and live generation changes.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs/promises');
const path=require('node:path');
const os=require('node:os');
const http=require('node:http');
const {execFileSync}=require('node:child_process');
let temporary,server,browser;
(async()=>{
 const root=path.resolve(__dirname,'../..');
 temporary=await fs.mkdtemp(path.join(os.tmpdir(),'cad-counts-'));
 const source=path.join(temporary,'main.star'),out=path.join(temporary,'out');
 async function build(points){
  await fs.writeFile(source,`b=revolve(id='body',profile=polygon(id='profile',points=${JSON.stringify(points)}))\np=part(id='pin',body=b)\noutput(name='solid',feature=b)\nassembly_output(name='pair',assembly=assembly(id='pair',instances=[instance(id='left',part=p),instance(id='right',part=p,offset=[30,0,0])]))\n`);
  execFileSync(process.env.CAD_BIN||path.join(root,'target/debug/cad'),['build',source,'--out-dir',out],{stdio:'pipe'});
 }
 await build([[0,0],[8,0],[0,12]]);
 let corrupt=false;
 server=http.createServer(async(req,res)=>{
  try{
   const name=new URL(req.url,'http://localhost').pathname;
   let data=await fs.readFile(path.join(out,name),'utf8');
   if(corrupt&&name.endsWith('solid.preview.html'))data=data.replace(/("native_faces":)\d+/, (_,prefix)=>prefix+'999');
   res.setHeader('Content-Type',name.endsWith('.html')?'text/html':'application/json');res.end(data);
  }catch{res.writeHead(404);res.end()}
 });
 await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
 browser=await chromium.launch({executablePath:process.env.CAD_BROWSER||undefined,args:['--no-sandbox']});
 const page=await browser.newPage({viewport:{width:1200,height:900}}),errors=[],exact=[];
 page.on('pageerror',e=>errors.push(e.message));page.on('request',r=>{if(/\.(brep\.json|step|stl)(\?|$)/.test(r.url()))exact.push(r.url())});
 const base=`http://127.0.0.1:${server.address().port}`;
 async function counts(native){
  await page.waitForFunction(expected=>[...document.querySelectorAll('#geometry dt')].some(dt=>dt.textContent.startsWith('Native faces')&&Number(dt.nextElementSibling.textContent)===expected),native);
  const rows=await page.evaluate(()=>Object.fromEntries([...document.querySelectorAll('#geometry dt')].map(dt=>[dt.textContent,dt.nextElementSibling.textContent])));
  assert.equal(Number(rows['Native faces']??rows['Native faces (sum)']),native);assert.ok(Number(rows['Display patches'])>native);assert.equal(rows.Faces,undefined);return rows;
 }
 await page.goto(base+'/solid.preview.html');await counts(2);
 await page.waitForFunction(()=>document.querySelector('.badge').textContent==='LIVE PREVIEW');
 const legacyAndMalformed=await page.evaluate(()=>{
  const legacy=JSON.parse(JSON.stringify(compactScene));delete legacy.native_faces;
  const old=normalizeScene(legacy);
  const bad=JSON.parse(JSON.stringify(compactScene));bad.native_faces=999;
  let rejected=false;try{normalizeScene(bad)}catch{rejected=true}
  return{legacy:old.native_faces,rejected};
 });
 assert.equal(legacyAndMalformed.legacy,2);assert.equal(legacyAndMalformed.rejected,true);
 // A malformed new generation must not replace the last complete scene.
 corrupt=true;await build([[0,0],[8,0],[8,20],[6,20],[0,32]]);
 await page.waitForFunction(()=>document.querySelector('.badge').textContent==='RECONNECTING');await counts(2);
 corrupt=false;await counts(4);
 await page.waitForFunction(()=>document.querySelector('.badge').textContent==='LIVE PREVIEW');
 assert.match(await fs.readFile(path.join(out,'solid.preview.svg'),'utf8'),/4 native faces/);
 await page.goto(base+'/pair.preview.html');await counts(8);
 const legacyAssembly=await page.evaluate(()=>{
  const legacy=JSON.parse(JSON.stringify(compactScene));
  for(const mesh of Object.values(legacy.meshes))delete mesh.native_faces;
  return normalizeScene(legacy).native_faces;
 });
 assert.equal(legacyAssembly,8);assert.deepEqual(errors,[]);assert.deepEqual(exact,[]);
 console.log(JSON.stringify({bodyNativeFaces:[2,4],assemblyNativeFaces:8,legacyCompatible:true,malformedRefreshRetainedPrevious:true,exactDownloads:0},null,2));
})().catch(e=>{console.error(e);process.exitCode=1}).finally(async()=>{
 if(browser)await browser.close();if(server)await new Promise(resolve=>server.close(resolve));
 if(temporary)await fs.rm(temporary,{recursive:true,force:true});
});
