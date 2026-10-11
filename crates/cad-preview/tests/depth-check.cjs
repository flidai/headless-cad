// Run after `npm ci --prefix tools/preview-check`:
// node crates/cad-preview/tests/depth-check.cjs
// Analytic ray/rectangle intersections independently check the triangle rasterizer.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const {pathToFileURL} = require('node:url');
const root = path.resolve(__dirname, '../../..');
const {chromium} = require(path.join(root, 'tools/preview-check/node_modules/playwright'));

function fixture() {
  const faces = [], crease_edges = [];
  const rings = [[0,3,2,1],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]];
  const normals = [[0,0,-1],[0,0,1],[0,-1,0],[1,0,0],[0,1,0],[-1,0,0]];
  // Disjoint bars whose projected depth ranges overlap substantially.
  const boxes = [
    {min:[0,0,0],max:[90,12,12]},
    {min:[35,-30,22],max:[47,70,34]},
    {min:[-12,25,-10],max:[110,37,2]},
  ];
  for (const {min,max} of boxes) {
    const [x,y,z] = min, [X,Y,Z] = max;
    const vertices = [[x,y,z],[X,y,z],[X,Y,z],[x,Y,z],[x,y,Z],[X,y,Z],[X,Y,Z],[x,Y,Z]];
    const edgeIds = new Map();
    rings.forEach((ring,index) => {
      const edges = ring.map((v,i) => {
        const key = [v,ring[(i+1)%4]].sort((a,b)=>a-b).join('/');
        if (!edgeIds.has(key)) { edgeIds.set(key,crease_edges.length); crease_edges.push(crease_edges.length); }
        return edgeIds.get(key);
      });
      faces.push({id:faces.length,points:ring.map(i=>vertices[i]),normal:normals[index],edges});
    });
  }
  return {faces,crease_edges,vertices:24,edges:36,solids:3,bounds:[[-12,-30,-10],[110,70,34]],volume:90*12*12+12*100*12+122*12*12};
}

(async () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(),'cad-depth-'));
  const file = path.join(directory,'occlusion.preview.html');
  fs.writeFileSync(file,fs.readFileSync(path.join(root,'crates/cad-preview/src/viewer.html'),'utf8').replace('/*SCENE_DATA*/null',JSON.stringify(fixture())));
  const browser = await chromium.launch({headless:true,executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox']});
  try {
    const page = await browser.newPage({viewport:{width:1100,height:850},deviceScaleFactor:2});
    const errors=[];page.on('pageerror',error=>errors.push(error.message));
    page.on('request',request=>{if(/^https?:/.test(request.url()))errors.push('Local artifact made a network request')});
    await page.goto(pathToFileURL(file).href);
    const result = await page.evaluate(() => {
      let samples=0,wrongPainterPixels=0,picks=0;
      const failures=[];
      for (const angle of [-.75,.3,1.2]) for (const elevation of [.3,.6,-.4]) {
        yaw=angle;pitch=elevation;draw();
        const right=[-Math.sin(yaw),Math.cos(yaw),0];
        const up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
        const direction=[Math.cos(yaw)*Math.cos(pitch),Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch)];
        const product=(a,b)=>a.reduce((sum,v,i)=>sum+v*b[i],0);
        const extent=[0,0];
        for(const face of scene.faces) for(const point of face.points) {
          const local=point.map((v,i)=>v-center[i]);
          extent[0]=Math.max(extent[0],Math.abs(product(local,right)));
          extent[1]=Math.max(extent[1],Math.abs(product(local,up)));
        }
        const scale=zoom*Math.min(Math.max(1,width-80)/(2*extent[0]||1),Math.max(1,height-150)/(2*extent[1]||1));
        let chosen=null;
        for(let py=2;py<bufferHeight;py+=11) for(let px=2;px<bufferWidth;px+=11) {
          const x=((px+.5)/renderRatio-width/2)/scale,y=-((py+.5)/renderRatio-height/2-20)/scale;
          const origin=center.map((v,i)=>v+right[i]*x+up[i]*y);
          const intersections=[];let nearBoundary=false;
          for(const face of scene.faces) {
            if(product(face.normal,direction)<=1e-10)continue;
            const axis=face.normal.findIndex(value=>value!==0);
            const distance=(face.points[0][axis]-origin[axis])/direction[axis];
            const hit=origin.map((v,i)=>v+distance*direction[i]);
            const inside=[0,1,2].filter(i=>i!==axis).every(i=>hit[i]>=Math.min(...face.points.map(p=>p[i]))&&hit[i]<=Math.max(...face.points.map(p=>p[i])));
            if(!inside)continue;
            for(const i of [0,1,2].filter(i=>i!==axis)) if(Math.min(...face.points.map(p=>Math.abs(p[i]-hit[i])))<1e-6)nearBoundary=true;
            const average=face.points.reduce((sum,p)=>sum+product(p.map((v,i)=>v-center[i]),direction)/face.points.length,0);
            intersections.push({face:face.id,distance,average});
          }
          if(nearBoundary||!intersections.length)continue;
          intersections.sort((a,b)=>b.distance-a.distance);
          const expected=intersections[0].face,actual=faceBuffer[py*bufferWidth+px];samples++;
          if(actual!==expected&&failures.length<8)failures.push({angle,elevation,px,py,expected,actual});
          const painter=intersections.slice().sort((a,b)=>b.average-a.average)[0].face;
          if(painter!==expected){wrongPainterPixels++;chosen={px,py,expected};}
        }
        if(chosen){select((chosen.px+.5)/renderRatio,(chosen.py+.5)/renderRatio);if(selected!==chosen.expected)throw Error('Picking disagrees with analytic nearest surface');picks++;}
      }
      return {samples,wrongPainterPixels,picks,failures};
    });
    assert.deepEqual(errors,[]);
    assert.deepEqual(result.failures,[]);
    assert.ok(result.samples>1000,'Expected substantial independent pixel coverage');
    assert.ok(result.wrongPainterPixels>0,'Fixture must expose the previous centroid painter defect');
    assert.ok(result.picks>0,'Expected nearest-surface picking coverage');
    console.log(JSON.stringify(result));
  } finally { await browser.close();fs.rmSync(directory,{recursive:true,force:true}); }
})().catch(error=>{console.error(error);process.exit(1)});
