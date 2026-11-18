// Authentic user project: read-only editorial check. Only regenerable playback
// cache/jobs may be created automatically by the app. No timeline/media writes.
'use strict';
const {chromium}=require('playwright');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),base='http://127.0.0.1:8088';
const projectId='b3b6d29f-56de-402e-8890-e89944d4a984';
const hash=p=>crypto.createHash('sha256').update(fs.readFileSync(p)).digest('hex');
async function main(){
 const record=path.join(root,'data/projects',projectId,'project.json');
 const before=hash(record),project=JSON.parse(fs.readFileSync(record,'utf8'));
 assert(project.timeline.clips.length>0,'Test requires existing user project; never create a timeline silently');
 const video=project.assets.find(a=>a.id===project.timeline.clips[0].asset_id);assert(video?.kind==='video');
 const media=path.join(root,'data/projects',projectId,'assets',video.file_name),sourceHash=hash(media);
 const browser=await chromium.launch({channel:'msedge',headless:true});
 try{
  const page=await browser.newPage({viewport:{width:1550,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base);await page.locator('#projects').selectOption(projectId);
  await page.locator('[data-nle-clip]').first().waitFor();
  await page.locator('#program-seek').fill('5');await page.locator('#program-seek').dispatchEvent('change');
  await page.waitForFunction(()=>{const canvas=document.querySelector('#program-canvas'),ctx=canvas.getContext('2d'),data=ctx.getImageData(0,0,canvas.width,canvas.height).data;let lo=255,hi=0;for(let i=0;i<data.length;i+=400){lo=Math.min(lo,data[i]);hi=Math.max(hi,data[i]);}return hi-lo>45&&document.querySelector('#program-status').hidden;},{},{timeout:180000});
  const bounds=await page.locator('#nle-scroll').boundingBox();assert(bounds.y<1000&&bounds.y+bounds.height<1010,'Timeline visible with monitor in 1550x1000 viewport');
  await page.locator('#program-play').click();await page.waitForFunction(()=>window.NLE.getTime()>5300);await page.locator('#program-play').click();
  const artifacts=path.join(__dirname,'artifacts');fs.mkdirSync(artifacts,{recursive:true});
  await page.locator(`[data-preview="${video.id}"]`).click();
  await page.waitForFunction(()=>{const v=document.querySelector('#preview');return v.readyState>=2&&v.videoWidth>0;},{},{timeout:180000});
  const decoded=await page.locator('#preview').evaluate(v=>({width:v.videoWidth,height:v.videoHeight,readyState:v.readyState,proxy:v.src.includes('/playback/file')}));assert(decoded.proxy,'Known HEVC user asset must use the compatible cache');assert(decoded.width<=960&&decoded.height<=720,'Proxy must stay within the lightweight budget in portrait too');
  // Pause triggers one last asynchronous seek. Capture only after it decodes,
  // with both monitors populated, not during the transient loading overlay.
  await page.waitForFunction(()=>document.querySelector('#program-status').hidden);
  const monitorBounds=await page.evaluate(()=>{const box=s=>{const r=document.querySelector(s).getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom,width:r.width,height:r.height};};return {well:box('.program-well'),program:box('#program-canvas'),source:box('#preview')};});
  for(const name of ['program','source']){const box=monitorBounds[name],well=monitorBounds.well;assert(box.width>30&&box.height>60,`${name} must have visible dimensions`);assert(box.left>=well.left-1&&box.right<=well.right+1&&box.top>=well.top-1&&box.bottom<=well.bottom+1,`${name} must remain inside the monitor after portrait source decoding: ${JSON.stringify(monitorBounds)}`);}
  await page.screenshot({path:path.join(artifacts,'prova-hevc-montaggio.png'),fullPage:true});
  assert.deepEqual(errors,[]);assert.equal(hash(record),before,'Editorial project record unchanged');assert.equal(hash(media),sourceHash,'Imported original unchanged');
  const report={passed:true,suite_version:(await(await fetch(base+'/api/health')).json()).version,project_id:projectId,real_user_media:true,checks:['HEVC browser decode via lightweight proxy','NLE canvas contains actual video frame','program playback advances','source monitor decodes video','timeline visible with monitor','zero JavaScript errors','project and imported original SHA-256 unchanged'],decoded,ai_services_started_by_test:false};
  fs.writeFileSync(path.join(artifacts,'real-playback-report.json'),JSON.stringify(report,null,2));
  console.log('PASS authentic Prova HEVC: visible NLE and source video, playback, original/project hashes unchanged; no AI service starts.');
 }finally{await browser.close();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
