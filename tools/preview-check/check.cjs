const { chromium } = require('playwright');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const preview = process.argv[2] || path.resolve(__dirname, '../../out/bracket/bracket.preview.html');
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CAD_BROWSER || undefined,args:['--no-sandbox']});
 const page=await browser.newPage({viewport:{width:1280,height:900},deviceScaleFactor:2});
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 page.on('request',request=>{if(/^https?:/.test(request.url()))errors.push('Unexpected network request: '+request.url())});
 await page.goto(pathToFileURL(path.resolve(preview)).href);
 await page.waitForTimeout(300);
 const modelBefore=await page.evaluate(()=>JSON.stringify(scene));
 if(process.env.CAD_SCREENSHOT) await page.screenshot({path:process.env.CAD_SCREENSHOT});
 const before=await page.locator('canvas').evaluate(c=>c.toDataURL());
 await page.getByRole('button',{name:'Top',exact:true}).click();
 const after=await page.locator('canvas').evaluate(c=>c.toDataURL());
 if(before===after)throw Error('Top view did not change rendering');
 await page.getByRole('button',{name:'Isometric',exact:true}).click();
 const box=await page.locator('canvas').boundingBox();
 let picked=false;
 for(let y=150;y<650&&!picked;y+=40)for(let x=200;x<750&&!picked;x+=40){await page.mouse.click(box.x+x,box.y+y);picked=(await page.locator('#selection').innerText()).startsWith('Face ')}
 if(!picked)throw Error('Face picking never selected visible geometry');
 await page.mouse.move(400,350);await page.mouse.down();await page.mouse.move(500,400,{steps:8});await page.mouse.up();
 await page.getByRole('button',{name:'Edges',exact:true}).click();
 if(await page.getByRole('button',{name:'Edges',exact:true}).getAttribute('aria-pressed')!=='false')throw Error('Edge toggle failed');
 await page.setViewportSize({width:390,height:844});

 if(await page.evaluate(()=>document.documentElement.scrollWidth>window.innerWidth))throw Error('Mobile overflow');
 if(errors.length)throw Error(errors.join('\n'));
 if(modelBefore!==await page.evaluate(()=>JSON.stringify(scene)))throw Error('Inspection modified the model');
 console.log(JSON.stringify({browser:'chromium',errors,picking:picked,view_changed:before!==after,mobile_overflow:false,read_only:true}));
 await browser.close();
})().catch(error=>{console.error(error);process.exit(1)});
