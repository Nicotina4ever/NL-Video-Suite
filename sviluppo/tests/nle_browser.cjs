'use strict';
// Synthetic-media UI regression, not a creative-quality or AI-generation benchmark.
// Run: node sviluppo/tests/nle_browser.cjs [path/to/nl-video-suite.exe]
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const assert=require('node:assert/strict'),http=require('node:http'),net=require('node:net');
const root=path.resolve(__dirname,'../..'),artifacts=path.join(__dirname,'artifacts');
const ffmpeg=path.join(root,'app','tools','ffmpeg.exe'),ffprobe=path.join(root,'app','tools','ffprobe.exe');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(action,predicate,label,timeout=20000){const deadline=Date.now()+timeout;let value;while(Date.now()<deadline){value=await action();if(predicate(value))return value;await delay(80);}throw Error(`${label}: timeout; last=${JSON.stringify(value)}`);}
async function freePort(){const listener=net.createServer();await new Promise(resolve=>listener.listen(0,'127.0.0.1',resolve));const port=listener.address().port;await new Promise(resolve=>listener.close(resolve));return port;}
function media(args){return execFileSync(ffmpeg,['-nostdin','-v','error','-n',...args],{windowsHide:true,maxBuffer:8*1024*1024});}
function pixel(buffer,width,x,y,height){const i=(Math.floor(y*height)*width+Math.floor(x*width))*3;return [...buffer.subarray(i,i+3)];}
function near(actual,expected,tolerance,label){assert(Math.abs(actual-expected)<=tolerance,`${label}: ${actual} vs ${expected}`);}

async function main(){
 const directory=fs.mkdtempSync(path.join(os.tmpdir(),'nl-nle-ui-'));
 const executable=process.argv[2]?path.resolve(process.argv[2]):path.join(root,'app','nl-video-suite.exe');
 const source=path.join(directory,'rosso sintetico.mp4'),green=path.join(directory,'verde grafica.png'),tone=path.join(directory,'tono 440Hz.wav');
 media(['-f','lavfi','-i','color=c=0xff0000:s=640x360:r=30:d=4','-an','-c:v','libx264','-pix_fmt','yuv420p',source]);
 media(['-f','lavfi','-i','color=c=0x00ff00:s=320x180:d=0.1','-frames:v','1','-c:v','png',green]);
 media(['-f','lavfi','-i','sine=frequency=440:sample_rate=48000:duration=4','-c:a','pcm_s16le',tone]);
 const fake=http.createServer((request,response)=>{response.setHeader('Content-Type','application/json');response.end(JSON.stringify(request.url==='/api/tags'?{models:[]}:request.url==='/object_info'?{}:{}));});
 await new Promise(resolve=>fake.listen(0,'127.0.0.1',resolve));
 const fakeUrl=`http://127.0.0.1:${fake.address().port}`,port=await freePort(),base=`http://127.0.0.1:${port}`;
 const data=path.join(directory,'data');fs.mkdirSync(data);
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:fakeUrl,ollama_url:fakeUrl,legacy_sources:{}}));
 let logs='',browser,page;const errors=[];
 const server=spawn(executable,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true,stdio:['ignore','pipe','pipe']});
 server.stdout.on('data',chunk=>{logs+=chunk;});server.stderr.on('data',chunk=>{logs+=chunk;});
 try{
  const health=await until(async()=>{try{const r=await fetch(base+'/api/health');return r.ok?await r.json():null;}catch{return null;}},Boolean,'server startup');
  assert(/^0\.[5-9]\./.test(health.version)||Number(health.version.split('.')[0])>0,`Executable ${health.version} is older than the NLE build; pass the current release path`);
  browser=await chromium.launch({channel:'msedge',headless:true});
  page=await browser.newPage({viewport:{width:1600,height:1500}});page.setDefaultTimeout(15000);
  page.on('pageerror',error=>errors.push(error.message));
  await page.goto(base);await page.locator('#new-project').click();
  await page.locator('#title').fill('Collaudo NLE · media sintetici');await page.locator('#create-form button').first().click();
  await page.waitForFunction(()=>document.querySelector('#notice').textContent==='Progetto creato.');
  const projectId=await page.locator('#projects').inputValue();
  const leftHandle=page.locator('[data-workspace-resize="left"]');
  const oldWidth=Number(await leftHandle.getAttribute('aria-valuenow'));
  await leftHandle.focus();await page.keyboard.press('ArrowRight');
  assert.equal(Number(await leftHandle.getAttribute('aria-valuenow')),oldWidth+10);
  const savedLayout=await page.evaluate(()=>JSON.parse(localStorage.getItem('nl-suite.workspace.v1')));
  console.log('NLE: importing synthetic video, image and audio');
  const getProject=async()=>{const r=await page.request.get(`${base}/api/projects/${projectId}`);assert(r.ok());return r.json();};
  const waitProject=async(predicate,label)=>{const result=await until(getProject,predicate,label);await page.waitForFunction(revision=>state.project?.updated_at===revision,result.updated_at);return result;};
  for(const [index,file]of [source,green,tone].entries()){
   await page.locator('#asset-path').fill(file);await page.locator('#import-form button').click();
   await page.waitForFunction(count=>document.querySelectorAll('[data-library-asset]').length===count,index+1,{timeout:25000});
  }
  let p=await getProject();const video=p.assets.find(a=>a.kind==='video'),image=p.assets.find(a=>a.kind==='image'),audio=p.assets.find(a=>a.kind==='audio');
  assert(video&&image&&audio);const videoTrack=p.timeline.tracks.find(t=>t.kind==='video'),imageTrack=p.timeline.tracks.find(t=>t.kind==='overlay');
  await page.locator(`[data-preview="${video.id}"]`).click();
  await page.waitForFunction(()=>document.querySelector('#preview').readyState>=2);
  await page.waitForFunction(()=>{const v=document.querySelector('#preview'),c=document.createElement('canvas');c.width=c.height=2;const ctx=c.getContext('2d');ctx.drawImage(v,0,0,2,2);return ctx.getImageData(0,0,1,1).data[0]>200;});
  assert(await page.locator('#program-canvas').isVisible()&&await page.locator('#preview').isVisible(),'source and program visible simultaneously');
  const sourceBounds=await page.locator('#preview').boundingBox(),programBounds=await page.locator('#program-canvas').boundingBox();
  assert(sourceBounds.x+sourceBounds.width<=programBounds.x+3,'source on left, program on right');
  await page.locator('#nle-scroll').focus();await page.keyboard.press('c');
  assert.match(await page.locator('#nle-blade-tool').getAttribute('class'),/selected-tool/);
  await page.keyboard.press('v');
  assert.match(await page.locator('#nle-select-tool').getAttribute('class'),/selected-tool/);
  assert.equal(await page.locator('#numeric-editor').getAttribute('open'),null,'numeric legacy editor remains closed by default');
  // Actual library drag-and-drop into V1; no direct API timeline setup.
  const lane=page.locator(`[data-lane="${videoTrack.id}"]`);
  console.log('NLE: library drag/drop');
  await page.locator(`[data-library-asset="${video.id}"]`).dragTo(lane,{sourcePosition:{x:100,y:18},targetPosition:{x:119,y:35}});
  p=await waitProject(p=>p.timeline.clips.length===1,'library drag/drop');
  let red=p.timeline.clips[0];near(red.start_ms,0,34,'drop at timeline origin');
  for(const id of [image.id,audio.id]){await page.locator(`[data-add="${id}"]`).click();await waitProject(p=>p.timeline.clips.some(c=>c.asset_id===id),'library add');}
  p=await getProject();const greenClip=p.timeline.clips.find(c=>c.asset_id===image.id),audioClip=p.timeline.clips.find(c=>c.asset_id===audio.id);
  assert.equal(greenClip.duration_ms,5000,'A single PNG must last five seconds, never ffprobe fake one-frame duration');
  // Both visual filmstrip tile and real FFmpeg waveform must load, not merely exist as CSS URLs.
  const loaded=[];
  for(const [id,kind]of [[red.id,'video'],[audioClip.id,'audio']]){
   const selector=`[data-nle-clip="${id}"] .clip-media`;
   const status=await page.locator(selector).evaluate(async element=>{const url=getComputedStyle(element).backgroundImage.match(/url\(["']?(.*?)["']?\)/)?.[1];if(!url)throw Error('missing clip media URL');const image=new Image();image.src=url;await image.decode();return{url,width:image.naturalWidth,height:image.naturalHeight,repeat:getComputedStyle(element).backgroundRepeat};});
   assert(status.width>0&&status.height>0);if(kind==='audio')assert(status.url.endsWith('/waveform'));else assert(status.url.endsWith('/thumbnail'));
   loaded.push({kind,...status});
  }
  await page.locator('#nle-snap').uncheck();
  console.log('NLE: pointer move and trims');
  async function gesture(selector,dx,edge){const block=page.locator(selector);await block.waitFor({state:'visible'});const box=await block.boundingBox();assert(box);const x=edge==='start'?box.x+3:edge==='end'?box.x+box.width-3:box.x+Math.min(box.width/2,40),y=box.y+32;await page.mouse.move(x,y);await page.mouse.down();await page.mouse.move(x+dx,y,{steps:8});await page.mouse.up();}
  async function select(id){await page.locator(`[data-nle-clip="${id}"]`).click({position:{x:20,y:20}});}
  await gesture(`[data-nle-clip="${red.id}"]`,30);
  p=await waitProject(p=>p.timeline.clips.find(c=>c.id===red.id).start_ms>=450,'move gesture');red=p.timeline.clips.find(c=>c.id===red.id);near(red.start_ms,500,34,'move +0.5 seconds');
  await gesture(`[data-nle-clip="${red.id}"]`,18,'start');
  p=await waitProject(p=>p.timeline.clips.find(c=>c.id===red.id).trim_in_ms>=250,'left trim gesture');red=p.timeline.clips.find(c=>c.id===red.id);
  near(red.start_ms,800,34,'trim start');near(red.trim_in_ms,300,34,'trim source in');near(red.duration_ms,3700,34,'trimmed duration');
  await gesture(`[data-nle-clip="${red.id}"]`,-30,'end');
  p=await waitProject(p=>p.timeline.clips.find(c=>c.id===red.id).duration_ms<3400,'right trim gesture');red=p.timeline.clips.find(c=>c.id===red.id);near(red.duration_ms,3200,34,'right trim duration');
  async function seekRuler(seconds){const ruler=page.locator('.nle-ruler');await ruler.scrollIntoViewIfNeeded();const box=await ruler.boundingBox();const pps=Number(await page.locator('#nle-zoom').inputValue());await page.mouse.click(box.x+seconds*pps,box.y+14);await page.waitForFunction(seconds=>Math.abs(window.NLE.getTime()/1000-seconds)<.06,seconds);}
  await select(red.id);await seekRuler(1.5);await page.locator('#nle-split').click();
  console.log('NLE: playhead split, undo and redo');
  p=await waitProject(p=>p.timeline.clips.filter(c=>c.asset_id===video.id).length===2,'split at playhead');
  const split=p.timeline.clips.filter(c=>c.asset_id===video.id).sort((a,b)=>a.start_ms-b.start_ms);
  near(split[0].duration_ms,700,35,'playhead split differs from midpoint');near(split[1].start_ms,1500,35,'second half starts at playhead');near(split[1].trim_in_ms,1000,35,'split source trim');
  await page.locator('#undo').click();await waitProject(p=>p.timeline.clips.filter(c=>c.asset_id===video.id).length===1,'undo split');
  await page.locator('#redo').click();await waitProject(p=>p.timeline.clips.filter(c=>c.asset_id===video.id).length===2,'redo split');
  await select(greenClip.id);
  console.log('NLE: inspector, track visibility and lock');
  for(const [field,value]of Object.entries({start_ms:'0.8',duration_ms:'3.2',x:'75',y:'50',scale:'40',opacity:'0.5'}))await page.locator(`[data-inspect="${field}"]`).fill(value);
  await page.locator('#inspector-apply').click();
  p=await waitProject(p=>p.timeline.clips.find(c=>c.id===greenClip.id).transform?.x===.75,'inspector transform save');
  const storedGreen=p.timeline.clips.find(c=>c.id===greenClip.id);assert.equal(storedGreen.transform.scale,.4);assert.equal(storedGreen.opacity,.5);
  // Track locks block actual pointer edits and disable inspector inputs.
  await page.locator(`[data-lane-lock="${imageTrack.id}"]`).click();await waitProject(p=>p.timeline.tracks.find(t=>t.id===imageTrack.id).locked,'lock track');
  await gesture(`[data-nle-clip="${greenClip.id}"]`,36);assert.deepEqual((await getProject()).timeline.clips.find(c=>c.id===greenClip.id),storedGreen);
  assert(await page.locator('[data-inspect="x"]').isDisabled());
  await page.locator('#nle-split').click();assert.match(await page.locator('#notice').innerText(),/Sblocca/);
  await page.locator(`[data-lane-lock="${imageTrack.id}"]`).click();await waitProject(p=>!p.timeline.tracks.find(t=>t.id===imageTrack.id).locked,'unlock track');
  await seekRuler(2);
  const canvasPixels=()=>page.locator('#program-canvas').evaluate(canvas=>{const c=canvas.getContext('2d');return{width:canvas.width,height:canvas.height,left:[...c.getImageData(Math.floor(canvas.width*.25),Math.floor(canvas.height*.5),1,1).data].slice(0,3),overlay:[...c.getImageData(Math.floor(canvas.width*.75),Math.floor(canvas.height*.5),1,1).data].slice(0,3)};});
  let preview=await until(canvasPixels,p=>p.left[0]>200&&p.left[1]<30&&p.overlay[0]>80&&p.overlay[1]>80,'canvas compositing',10000);
  await page.locator(`[data-lane-visible="${imageTrack.id}"]`).click();await waitProject(p=>!p.timeline.tracks.find(t=>t.id===imageTrack.id).visible,'hide graphics track');
  await until(canvasPixels,p=>p.overlay[0]>200&&p.overlay[1]<30,'hidden graphics absent in preview');
  await page.locator(`[data-lane-visible="${imageTrack.id}"]`).click();await waitProject(p=>p.timeline.tracks.find(t=>t.id===imageTrack.id).visible,'show graphics track');
  // Reload from disk; persistent transforms and pointer edits must survive.
  await page.reload();await page.locator(`[data-nle-clip="${greenClip.id}"]`).waitFor();
  assert.equal(Number(await leftHandle.getAttribute('aria-valuenow')),savedLayout.left,'workspace width persisted');
  p=await getProject();assert.deepEqual(p.timeline.clips.find(c=>c.id===greenClip.id),storedGreen);
  await seekRuler(2);preview=await until(canvasPixels,p=>p.left[0]>200&&p.left[1]<30&&p.overlay[0]>80&&p.overlay[1]>80,'persisted video AND overlay preview');
  await page.locator('#program-play').click();await page.waitForFunction(()=>window.NLE.getTime()>2200);await page.locator('#program-play').click();
  await seekRuler(2);
  const request=page.waitForResponse(response=>response.url()===`${base}/api/projects/${projectId}/export`&&response.request().method()==='POST');await page.locator('#render').click();const exportJob=await (await request).json();
  console.log('NLE: real export and preview pixel comparison');
  const job=await until(async()=>{const response=await page.request.get(`${base}/api/jobs/${exportJob.id}`);return response.json();},j=>['failed','succeeded'].includes(j.status),'real FFmpeg export',90000);assert.equal(job.status,'succeeded',job.message);
  const output=job.output_paths[0];assert(fs.existsSync(output));
  const info=JSON.parse(execFileSync(ffprobe,['-v','error','-show_streams','-of','json',output],{windowsHide:true,encoding:'utf8'}));assert(info.streams.some(s=>s.codec_type==='audio'));
  const frame=media(['-ss','2','-i',output,'-vf','scale=960:540','-frames:v','1','-pix_fmt','rgb24','-f','rawvideo','pipe:1']);
  const rendered={left:pixel(frame,960,.25,.5,540),overlay:pixel(frame,960,.75,.5,540)};
  for(const point of ['left','overlay'])for(let channel=0;channel<3;channel++)near(rendered[point][channel],preview[point][channel],28,`preview/export pixel ${point} channel ${channel}`);
  const pcm=media(['-ss','2','-i',output,'-t','0.15','-vn','-ac','1','-ar','48000','-f','s16le','pipe:1']);let energy=0;for(let i=0;i<pcm.length;i+=2)energy+=pcm.readInt16LE(i)**2;assert(Math.sqrt(energy/(pcm.length/2))>500,'export contains real tone, not silence');
  // A job finishing while numeric editing has focus must refresh AFTER editing,
  // not disappear forever from the update detector (a former workflow stall).
  await page.locator('#numeric-editor summary').click();await page.locator('#timeline [data-field="start_ms"]').first().focus();
  const extra=await(await page.request.post(`${base}/api/projects/${projectId}/assets`,{data:{path:green}})).json();
  await until(async()=>{const r=await page.request.get(`${base}/api/jobs/${extra.id}`);return r.json();},j=>j.status==='succeeded','background import during editing');
  await page.waitForTimeout(3000);assert.equal(await page.locator('[data-library-asset]').count(),3,'Do not replace controls during editing');
  await page.locator('#monitor-program').click();await page.waitForFunction(()=>document.querySelectorAll('[data-library-asset]').length===4);
  await page.locator('#numeric-editor summary').click();
  for(const kind of ['audio','overlay']){await page.locator('#nle-track-kind').selectOption(kind);await page.locator('#nle-add-track').click();await waitProject(p=>p.timeline.tracks.filter(t=>t.kind===kind).length===2,'additional track '+kind);}
  fs.mkdirSync(artifacts,{recursive:true});await select(greenClip.id);await seekRuler(2);
  await page.locator('#monitor-dual').click();
  await page.screenshot({path:path.join(artifacts,'nle-desktop.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});await page.locator('#program-canvas').scrollIntoViewIfNeeded();
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth+1),'NLE mobile document overflow');
  await page.screenshot({path:path.join(artifacts,'nle-mobile.png'),fullPage:true});
  assert.deepEqual(errors,[],'no uncaught JavaScript errors');
  const report={passed:true,version:health.version,synthetic:true,ai_services:'FAKE: metadata only; no inference',checks:['library drag/drop','filmstrip and waveform decoded','move and both trim handles','non-midpoint playhead split','undo redo','inspector transforms and opacity','track visibility and lock','reload persistence','playback','real FFmpeg export and PCM tone','canvas/export RGB samples','mobile no document overflow'],preview,rendered,loaded};
  fs.writeFileSync(path.join(artifacts,'nle-report.json'),JSON.stringify(report,null,2));
  fs.writeFileSync(path.join(artifacts,'nle-LEGGIMI.md'),'# Collaudo sintetico NLE\n\nEseguire `node sviluppo/tests/nle_browser.cjs [eseguibile]`.\n\nVideo rosso, grafica verde e tono sono generati dal test. Export FFmpeg reale; servizi IA simulati solo per metadati. Non verifica qualità artistica, workflow ComfyUI reali o prestazioni su un altro PC.\n\nScreenshot: nle-desktop.png e nle-mobile.png. Dettagli e campioni RGB: nle-report.json.\n');
  console.log('PASS NLE browser: drag/drop, trim/move, playhead split, undo/redo, compositing, lock, persistence, FFmpeg pixel/audio parity, mobile. AI services FAKE.');
 }catch(error){fs.mkdirSync(artifacts,{recursive:true});if(page)await page.screenshot({path:path.join(artifacts,'nle-failure.png'),fullPage:true}).catch(()=>{});fs.writeFileSync(path.join(artifacts,'nle-failure.log'),logs);throw error;
 }finally{
  if(browser)await browser.close();
  if(server.exitCode===null){server.kill();await new Promise(resolve=>server.once('exit',resolve));}
  await new Promise(resolve=>fake.close(resolve));
  assert.equal(path.dirname(directory),path.resolve(os.tmpdir()));assert(path.basename(directory).startsWith('nl-nle-ui-'));
  fs.rmSync(directory,{recursive:true,force:true,maxRetries:10,retryDelay:100});
 }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
