const {navigate}=require('./navigation.cjs');
'use strict';
// Regression for the reported interactions. Only synthetic media / local fake AI.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),ffmpeg=path.join(root,'app/tools/ffmpeg.exe');
const delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,test,label){const end=Date.now()+35000;let v;while(Date.now()<end){v=await fn();if(test(v))return v;await delay(120);}throw Error(label+': '+JSON.stringify(v));}
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function main(){
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'nl-studio-revision-')),data=path.join(dir,'data'),workflows=path.join(dir,'workflows');fs.mkdirSync(data);fs.mkdirSync(workflows);
 const video=path.join(dir,'source.mp4'),audio=path.join(dir,'music.wav'),visual=path.join(dir,'visual.png');
 const ff=args=>execFileSync(ffmpeg,['-v','error','-nostdin','-n',...args],{windowsHide:true,maxBuffer:16*1024*1024});
 ff(['-f','lavfi','-i','color=red:s=640x360:r=30:d=8','-c:v','libx264','-threads','2',video]);ff(['-f','lavfi','-i','sine=frequency=440:duration=2','-c:a','pcm_s16le',audio]);
 ff(['-f','lavfi','-i','color=0x4b9cab:s=320x240','-frames:v','1','-threads','2',visual]);
 const graph={positive:{class_type:'CLIPTextEncode',inputs:{text:'original'}},save:{class_type:'SaveAudio',inputs:{filename_prefix:'fixture'}},image:{class_type:'LoadImage',inputs:{image:'reference.png'}}};
 fs.writeFileSync(path.join(workflows,'fixture-audio.json'),JSON.stringify(graph));
 fs.writeFileSync(path.join(workflows,'fixture-graphic.json'),JSON.stringify({positive:graph.positive,save:{class_type:'SaveImage',inputs:{filename_prefix:'fixture-image'}}}));
 fs.writeFileSync(path.join(workflows,'fixture-i2v.json'),JSON.stringify({positive:graph.positive,image:graph.image,save:{class_type:'SaveVideo',inputs:{filename_prefix:'fixture-video'}}}));
 let released=false,submitted=false,uploaded=false,received;
 const fake=http.createServer(async(req,res)=>{const chunks=[];for await(const b of req)chunks.push(b);const bytes=Buffer.concat(chunks);let value={};
  if(req.url==='/object_info')value={CLIPTextEncode:{input:{required:{text:['STRING']}},output:['CONDITIONING']},SaveAudio:{input:{required:{filename_prefix:['STRING']}},output_node:true,output:[]},SaveImage:{input:{required:{filename_prefix:['STRING']}},output_node:true,output:[]},SaveVideo:{input:{required:{filename_prefix:['STRING']}},output_node:true,output:[]},LoadImage:{input:{required:{image:[['reference.png','uploaded.png'],{image_upload:true}]}},output:['IMAGE','MASK']}};
  else if(req.url==='/api/tags')value={models:[]};
  else if(req.url==='/upload/image'){uploaded=true;assert(bytes.includes(Buffer.from('PNG')));value={name:'uploaded.png',subfolder:''};}
  else if(req.url==='/prompt'){received=JSON.parse(bytes);submitted=true;value={prompt_id:received.prompt.save.class_type==='SaveImage'?'image-test':received.prompt.save.class_type==='SaveVideo'?'video-test':'sound-test'};}
  else if(req.url==='/queue')value={queue_running:submitted&&!released?[[0,'sound-test']]:[],queue_pending:[]};
  else if(req.url.startsWith('/history/')){const id=req.url.split('/').pop(),image=id==='image-test',vid=id==='video-test';value=released?{[id]:{status:{completed:true,status_str:'success'},outputs:{save:{[image?'images':vid?'gifs':'audio']:[{filename:image?'visual.png':vid?'generated.mp4':'music.wav',type:'output',subfolder:''}]}}}}:{};}
  else if(req.url.startsWith('/view?')){res.end(fs.readFileSync(req.url.includes('visual.png')?visual:req.url.includes('generated.mp4')?video:audio));return;}
  res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));
 });await new Promise(r=>fake.listen(0,'127.0.0.1',r));const fakeUrl='http://127.0.0.1:'+fake.address().port;
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:fakeUrl,ollama_url:fakeUrl,legacy_sources:{},workflow_roots:[workflows]}));
 const port=await freePort(),base='http://127.0.0.1:'+port,exe=path.resolve(process.argv[2]||path.join(root,'sviluppo/target/release/nl-video-suite.exe'));
 const server=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,windowsHide:true,stdio:['ignore','pipe','pipe'],env:{...process.env,NL_SUITE_ROOT:root}});let logs='',browser,page;server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
 const api=async(route,method='GET',body,status=200)=>{const r=await fetch(base+route,{method,headers:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});assert.equal(r.status,status,await r.clone().text());return r.json();};
 try{
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}},Boolean,'startup');
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1700,height:1250}});page.setDefaultTimeout(20000);const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('dialog',d=>d.accept());
  await page.goto(base);await page.locator('#new-project').click();await page.locator('#title').fill('Regression studio');await page.locator('#create-form button').first().click();await page.waitForFunction(()=>state.project?.title==='Regression studio');
  const pid=await page.locator('#projects').inputValue(),prefix='/api/projects/'+pid;
  const get=()=>api(prefix),waitP=(fn,label)=>until(get,fn,label);
  const imp=await api(prefix+'/assets','POST',{path:video},202);await until(()=>api('/api/jobs/'+imp.id),j=>j.status==='succeeded','import');await page.locator('#reload').click();
  let p=await get();const source=p.assets[0];await page.locator(`[data-add="${source.id}"]`).click();await waitP(p=>p.timeline.clips.length===1,'clip');
  await page.locator('#nle-new-object').click();p=await waitP(p=>p.graphic_objects.length===1,'object');const oid=p.graphic_objects[0].id;
  const obj=page.locator(`[data-nle-object="${oid}"]`);await obj.scrollIntoViewIfNeeded();let box=await obj.boundingBox();
  await page.mouse.move(box.x+30,box.y+18);await page.mouse.down();await page.mouse.move(box.x+150,box.y+18,{steps:12});await page.mouse.up();
  p=await waitP(p=>p.graphic_objects[0].start_ms>=1900,'real object drag');assert(p.graphic_objects[0].start_ms<=2100);
  await obj.scrollIntoViewIfNeeded();box=await obj.boundingBox();await page.mouse.move(box.x+box.width-3,box.y+25);await page.mouse.down();await page.mouse.move(box.x+box.width+57,box.y+25,{steps:10});await page.mouse.up();
  p=await waitP(p=>p.graphic_objects[0].duration_ms>=3900,'real object trim');
  await page.locator('#object-media-kind').selectOption('audio');await page.locator('#object-prompt').fill('Subtle instrumental sound design supporting a calm explanation, no vocals.');
  const wf=(await api('/api/workflows')).find(w=>w.name==='fixture-audio');await page.locator('#object-workflow').selectOption(wf.id);await page.locator('#object-text-input').selectOption('positive.text');
  await page.locator('#program-seek').fill('2.5');await page.locator('#program-seek').dispatchEvent('change');await page.locator('#object-frame').click();
  p=await waitP(p=>p.assets.some(a=>a.name.includes('frame')),'frame extracted');const frame=p.assets.find(a=>a.name.includes('frame'));assert.equal(frame.media.width,640);assert.equal(frame.media.height,360);
  await until(()=>page.locator('[data-object-reference="image.image"]').inputValue(),v=>v===frame.id,'frame binding selected');
  await navigate(page,'workflows');await page.locator('#generation-ai-audio').click();
  await page.locator('#object-generate').click();await until(async()=>submitted,Boolean,'Comfy accepted');assert(uploaded);assert.match(received.prompt.positive.inputs.text,/instrumental/);
  await page.locator('#creative-close').click();
  const jobs=await api('/api/jobs'),active=jobs.filter(j=>['running','queued'].includes(j.status));assert(active.length>=1);
  await navigate(page,'graphics');await page.waitForFunction(()=>document.querySelector('#graphic-review').textContent.includes('lavorazione'));
  assert(!await page.locator('#graphic-review').textContent().then(t=>t.includes('Nessuna nuova')));
  await navigate(page,'workflows');await page.waitForFunction(()=>document.querySelector('#tab-workflows .project-activity').textContent.includes('ComfyUI in esecuzione'));
  await navigate(page,'jobs');await page.locator('#archive-jobs').click();const after=await api('/api/jobs');assert(after.some(j=>j.archived));for(const j of active)assert.equal(after.find(x=>x.id===j.id).archived,false);
  released=true;p=await waitP(p=>p.graphic_objects[0].status==='review','audio review');assert.equal(p.timeline.clips.length,1);
  await page.waitForFunction(()=>document.querySelector('#job-notifications').textContent.includes('Completato'));
  await navigate(page,'workflows');await page.locator('#generation-ai-audio').click();await page.locator('#object-review-audio').waitFor();await page.waitForFunction(()=>document.querySelector('#object-review-audio').readyState>=2);
  await page.locator('#object-approve').click();p=await waitP(p=>p.timeline.clips.length===2,'audio in timeline');const audioClip=p.timeline.clips.find(c=>p.assets.find(a=>a.id===c.asset_id)?.kind==='audio');assert(audioClip);assert.equal(audioClip.duration_ms,2000);assert(audioClip.volume>0);assert.equal(p.timeline.tracks.find(t=>t.id===audioClip.track_id).kind,'audio');
  await page.locator('#creative-close').click();await navigate(page,'studio');
  audioClip.start_ms+=500;audioClip.duration_ms=1500;
  p=await api(prefix+'/timeline','PUT',{timeline:p.timeline,expected_updated_at:p.updated_at});
  assert.equal(p.graphic_objects[0].start_ms,audioClip.start_ms);assert.equal(p.graphic_objects[0].duration_ms,1500);
  p=await api(prefix+'/graphic-objects/'+oid,'PUT',{prompt:'Edited prompt after moving the approved clip'});
  assert.equal(p.timeline.clips.find(c=>c.id===audioClip.id).start_ms,audioClip.start_ms,'prompt save must not undo a timeline move');
  await page.locator('#reload').click();
  // Explicit audio panel: an existing Comfy output can be uploaded, auditioned
  // and placed without knowing the generic object inspector or re-generating.
  const priorAudioCount=p.assets.filter(a=>a.kind==='audio').length;
  await page.locator('#nle-ai-audio').click();await page.locator('#creative-dialog').waitFor();assert.equal(await page.locator('#object-media-kind').inputValue(),'audio');
  await page.locator('#creative-audio-file').setInputFiles(audio);await page.locator('#creative-audio-import').click();
  p=await waitP(p=>p.assets.filter(a=>a.kind==='audio').length>priorAudioCount,'existing audio import');
  await page.waitForFunction(n=>document.querySelectorAll('[data-creative-audio]').length>n,priorAudioCount);await page.waitForFunction(()=>!document.querySelector('#creative-audio-import').disabled);
  await page.waitForFunction(()=>[...document.querySelectorAll('#creative-audios audio')].some(a=>a.readyState>=2));
  await page.locator('#creative-audio-at').fill('6');await page.locator('#creative-audio-volume').fill('0.15');await page.locator('[data-creative-audio]').last().click();
  p=await waitP(p=>p.timeline.clips.some(c=>c.start_ms===6000&&c.volume===.15),'audio explicit insert');
  assert(await page.locator('#creative-dialog').evaluate(el=>el.scrollWidth<=el.clientWidth+1));
  fs.mkdirSync(path.join(__dirname,'artifacts'),{recursive:true});await page.screenshot({path:path.join(__dirname,'artifacts/studio-audio-panel.png'),fullPage:true});
  await page.locator('#creative-close').click();
  // The visual entry point must use a real workflow request, not a local card.
  await page.locator('#nle-ai-graphic').click();await page.locator('#creative-dialog').waitFor();assert.equal(await page.locator('#object-media-kind').inputValue(),'image');assert.equal(await page.locator('#object-prompt').inputValue(),'');assert.equal(await page.locator('#object-description').inputValue(),'');assert.equal(await page.locator('#card-dialog').count(),0);
  await page.locator('#object-title').fill('Visual originale IA');await page.locator('#object-description').fill('Materiali traslucidi, composizione asimmetrica.');await page.locator('#object-prompt').fill('Subject: an original translucent waveform, asymmetric composition, no text, no logos.');
  const imageWf=(await api('/api/workflows')).find(w=>w.name==='fixture-graphic');await page.locator('#object-workflow').selectOption(imageWf.id);await page.locator('#object-text-input').selectOption('positive.text');await page.locator('#object-generate').click();
  p=await waitP(p=>p.graphic_objects.some(o=>o.title==='Visual originale IA'&&o.status==='review'),'AI image review');const graphic=p.graphic_objects.find(o=>o.title==='Visual originale IA');assert.match(received.prompt.positive.inputs.text,/translucent waveform/);assert.equal(received.prompt.save.class_type,'SaveImage');
  await page.waitForFunction(()=>document.querySelector('#object-review-image').naturalWidth===320);await page.locator('#object-approve').click();p=await waitP(p=>p.timeline.clips.some(c=>c.label==='Visual originale IA'),'generated visual inserted');
  await page.screenshot({path:path.join(__dirname,'artifacts/studio-ai-graphic-panel.png'),fullPage:true});await page.locator('#creative-close').click();
  const card=p.timeline.clips.find(c=>c.label==='Visual originale IA');await page.locator(`[data-nle-clip="${card.id}"]`).click();await page.locator('#clip-motion').selectOption('slide_left');p=await waitP(p=>p.timeline.clips.find(c=>c.id===card.id).transform.motion==='slide_left','generated visual animation');card.transform.motion='slide_left';
  // A real source frame must reach the explicitly chosen I2V graph; a decoded
  // fake result is then reviewed and inserted on a visual, never audio, track.
  const frameCount=p.assets.filter(a=>a.source_path?.startsWith('Frame di asset ')).length;
  await page.evaluate(()=>NLE.seek(3500));await page.locator('#nle-ai-frame').click();await page.locator('#creative-dialog').waitFor();
  p=await waitP(p=>p.assets.filter(a=>a.source_path?.startsWith('Frame di asset ')).length>frameCount,'I2V frame captured');
  await page.locator('#object-frame-reference').waitFor();await page.waitForFunction(()=>!document.querySelector('#object-frame').disabled);
  const videoObject=p.graphic_objects.find(o=>o.media_kind==='video'),i2v=(await api('/api/workflows')).find(w=>w.name==='fixture-i2v');
  await page.locator('#object-workflow').selectOption(i2v.id);await page.locator('#object-text-input').selectOption('positive.text');
  await until(()=>page.locator('[data-object-reference="image.image"]').inputValue(),v=>Boolean(v),'captured frame bound');
  await page.locator('#object-prompt').fill('Preserve the source frame identity, subtle slow camera push, no new text.');await page.locator('#object-generate').click();
  p=await waitP(p=>p.graphic_objects.find(o=>o.id===videoObject.id)?.status==='review','I2V review');assert.equal(received.prompt.image.inputs.image,'uploaded.png');assert.equal(received.prompt.save.class_type,'SaveVideo');
  await page.waitForFunction(()=>document.querySelector('#object-review-video').readyState>=2);await page.locator('#object-approve').click();p=await waitP(p=>p.graphic_objects.find(o=>o.id===videoObject.id)?.status==='approved','I2V insertion');
  const videoAsset=p.graphic_objects.find(o=>o.id===videoObject.id).asset_id,videoClip=p.timeline.clips.find(c=>c.asset_id===videoAsset);assert(videoClip);assert.equal(p.timeline.tracks.find(t=>t.id===videoClip.track_id).kind,'overlay');assert.equal(videoClip.start_ms,3500);
  await page.screenshot({path:path.join(__dirname,'artifacts/studio-i2v-panel.png'),fullPage:true});await page.locator('#creative-close').click();
  const timeline=p.timeline;timeline.clips=[{...card,start_ms:0,duration_ms:2000,transform:{...card.transform,x:.5}}];await api(prefix+'/timeline','PUT',{timeline,expected_updated_at:p.updated_at});await page.locator('#reload').click();
  await page.evaluate(()=>NLE.seek(700));await page.waitForTimeout(300);
  const pixels=await page.locator('#program-canvas').evaluate(canvas=>{const d=canvas.getContext('2d').getImageData(0,0,canvas.width,canvas.height).data;return d.reduce((sum,v,i)=>sum+(i%4===0?v:0),0);});assert(pixels>100000,'card visible in actual program preview');
  const exported=await api(prefix+'/export','POST',undefined,202);const done=await until(()=>api('/api/jobs/'+exported.id),j=>['succeeded','failed'].includes(j.status),'animated export');assert.equal(done.status,'succeeded',done.message);
  const mean=sec=>{const raw=ff(['-ss',String(sec),'-i',done.output_paths[0],'-frames:v','1','-vf','scale=32:18','-pix_fmt','rgb24','-f','rawvideo','-']);assert(raw.length>0,'decoded export frame');return raw.reduce((a,b)=>a+b,0)/raw.length;};const first=mean(0),middle=mean(.7),last=mean(1.9);assert(middle>first+10&&middle>last+10,JSON.stringify({first,middle,last}));
  await navigate(page,'system');assert.equal(await page.locator('#legacy-import-panel').count(),0);assert.deepEqual(errors,[]);
  await page.setViewportSize({width:390,height:844});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
  await navigate(page,'workflows');await page.locator('#generation-ai-audio').click();assert(await page.locator('#creative-dialog').evaluate(el=>el.scrollWidth<=el.clientWidth+1),'mobile creative dialog must not clip fields');await page.locator('#creative-close').click();
  fs.mkdirSync(path.join(__dirname,'artifacts'),{recursive:true});fs.writeFileSync(path.join(__dirname,'artifacts/studio-revision-report.json'),JSON.stringify({passed:true,synthetic:true,ai_inference:false,checks:['object mouse move/trim','full-resolution frame reference upload','audio output review/approval on audio track','explicit audio panel/import/listen/insert','active job stages','notifications','archive preserves active/outputs','AI graphic workflow with no local template','frame -> I2V binding -> video review -> visual track','animated preview and FFmpeg fade pixels','legacy menu removed','mobile'],animation_pixels:{first,middle,last}},null,2));
  console.log('PASS studio revision: object drag/trim, frame reference, audio workflow review/import/insert, statuses/archive, AI graphic via simulated Comfy and animated FFmpeg pixels; no real AI inference.');
 }catch(error){if(page){fs.mkdirSync(path.join(__dirname,'artifacts'),{recursive:true});await page.screenshot({path:path.join(__dirname,'artifacts/studio-revision-failure.png'),fullPage:true});}console.error(logs);throw error;}
 finally{released=true;if(browser)await browser.close();server.kill();await new Promise(r=>server.exitCode!==null?r():server.once('exit',r));await new Promise(r=>fake.close(r));if(path.dirname(dir)===os.tmpdir()&&path.basename(dir).startsWith('nl-studio-revision-'))fs.rmSync(dir,{recursive:true,force:true,maxRetries:5,retryDelay:100});}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
