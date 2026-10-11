// Independent analytic prism geometry, depth and picking oracle for tilted_jig.
const {chromium} = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const {pathToFileURL} = require('node:url');

(async () => {
  const directory = process.env.CAD_TRANSFORM_DIR || path.resolve(__dirname, '../../out/tilted_jig');
  const browser = await chromium.launch({executablePath: process.env.CAD_BROWSER || undefined, args: ['--no-sandbox']});
  try {
    const page = await browser.newPage({viewport: {width: 1100, height: 850}, deviceScaleFactor: 2});
    const errors = []; page.on('pageerror', error => errors.push(error.message));
    const results = [];
    for (const [name, sign, offset] of [['left_tool', 1, [10,20,4]], ['right_tool', -1, [40,30,4]]]) {
      await page.goto(pathToFileURL(path.join(directory, `${name}.preview.html`)).href);
      const result = await page.evaluate(({sign, offset}) => {
        const dot = (a,b) => a.reduce((sum,v,i) => sum + v*b[i],0);
        const sub = (a,b) => a.map((v,i) => v-b[i]);
        const cross = (a,b) => [a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]];
        const unit = a => { const norm=Math.hypot(...a); return a.map(v=>v/norm); };
        const c=Math.sqrt(3)/2,s=sign/2;
        // Exact Rx90 then Rz±30, independent of quaternion/kernel implementation.
        const vertices=[[0,0,0],[30,0,0],[0,12,0],[0,0,8],[30,0,8],[0,12,8]]
          .map(([x,y,z])=>[offset[0]+c*x+s*z,offset[1]+s*x-c*z,offset[2]+y]);
        const rings=[[0,2,1],[3,4,5],[0,1,4,3],[1,2,5,4],[2,0,3,5]];
        const faces=rings.map(ring=>{
          const points=ring.map(i=>vertices[i]);
          const normal=unit(cross(sub(points[1],points[0]),sub(points[2],points[0])));
          const native=scene.faces.find(face=>face.points.length===points.length && points.every(p=>face.points.some(q=>Math.hypot(...sub(p,q))<1e-9)));
          if(!native || Math.abs(dot(normal,native.normal)-1)>1e-12) throw Error('Native transformed plane differs from analytic prism');
          return {points,normal,id:native.id};
        });
        if(scene.faces.length!==5) throw Error('Prism topology changed');
        let samples=0,hits=0,empty=0,picks=0;
        for(const angle of [-.9,.35,1.2]) for(const elevation of [-.3,.5]) {
          yaw=angle;pitch=elevation;draw();
          const right=[-Math.sin(yaw),Math.cos(yaw),0];
          const up=[-Math.cos(yaw)*Math.sin(pitch),-Math.sin(yaw)*Math.sin(pitch),Math.cos(pitch)];
          const direction=[Math.cos(yaw)*Math.cos(pitch),Math.sin(yaw)*Math.cos(pitch),Math.sin(pitch)];
          const extents=[0,0];
          for(const vertex of vertices){const p=sub(vertex,center);extents[0]=Math.max(extents[0],Math.abs(dot(p,right)));extents[1]=Math.max(extents[1],Math.abs(dot(p,up)));}
          const scale=zoom*Math.min(Math.max(1,width-80)/(2*extents[0]||1),Math.max(1,height-150)/(2*extents[1]||1));
          let chosen=null;
          for(let py=3;py<bufferHeight;py+=13) for(let px=3;px<bufferWidth;px+=13) {
            const x=((px+.5)/renderRatio-width/2)/scale,y=-((py+.5)/renderRatio-height/2-20)/scale;
            const origin=center.map((v,i)=>v+right[i]*x+up[i]*y);
            const intersections=[];let boundary=false;
            for(const face of faces){
              const denominator=dot(face.normal,direction);if(denominator<=1e-10)continue;
              const distance=dot(face.normal,sub(face.points[0],origin))/denominator;
              const hit=origin.map((v,i)=>v+distance*direction[i]);
              const sides=face.points.map((p,i)=>dot(cross(sub(face.points[(i+1)%face.points.length],p),sub(hit,p)),face.normal));
              if(sides.some(v=>Math.abs(v)<1e-7))boundary=true;
              if(sides.every(v=>v>=0))intersections.push({distance,id:face.id});
            }
            if(boundary)continue;
            intersections.sort((a,b)=>b.distance-a.distance);
            const expected=intersections.length?intersections[0].id:-1;
            const actual=faceBuffer[py*bufferWidth+px];
            if(actual!==expected)throw Error(`Analytic transformed depth mismatch: ${px},${py}, expected ${expected}, got ${actual}`);
            samples++;if(expected<0)empty++;else{hits++;chosen={px,py,expected};}
          }
          if(chosen){select((chosen.px+.5)/renderRatio,(chosen.py+.5)/renderRatio);if(selected!==chosen.expected)throw Error('Transformed face picking mismatch');picks++;}
        }
        return {samples,hits,empty,picks,analytic_planes:faces.length};
      }, {sign,offset});
      assert.ok(result.hits>100 && result.empty>100 && result.picks===6);
      results.push({name,...result});
    }
    assert.deepEqual(errors,[]);
    console.log(JSON.stringify({valid:true,results,errors}));
  } finally { await browser.close(); }
})().catch(error=>{console.error(error);process.exitCode=1;});
