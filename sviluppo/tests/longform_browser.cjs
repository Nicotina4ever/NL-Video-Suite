const {navigate}=require('./navigation.cjs');
'use strict';
// Canonical raw/corrected text -> reversible source cut -> common timeline -> MP4.
const {chromium}=require('playwright'),{spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,p=Boolean,label='wait',ms=30000){let value;const end=Date.now()+ms;while(Date.now()<end){value=await fn();if(p(value))return value;await sleep(100);}throw Error(label+': '+JSON.stringify(value));}
async function main(){
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'nl-longform-')),data=path.join(dir,'data');fs.mkdirSync(data);fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:'http://127.0.0.1:9',ollama_url:'http://127.0.0.1:9',legacy_sources:{}}));
 const source=path.join(dir,'voce à italiana.wav'),ffmpeg=path.join(root,'app/tools/ffmpeg.exe');execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','sine=frequency=440:duration=10',source]);
 const probe=net.createServer();await new Promise(r=>probe.listen(0,'127.0.0.1',r));const port=probe.address().port;await new Promise(r=>probe.close(r));const base='http://127.0.0.1:'+port,exe=process.argv[2]||path.join(root,'sviluppo/target/release/nl-video-suite.exe');
 let server,logs='',browser,page;function launch(){server=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);}
 const api=async(t,m='GET',b,status=200)=>{const r=await fetch(base+t,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined});const v=await r.json();assert.equal(r.status,status,JSON.stringify(v));return v;};
 async function ready(){await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});}
 async function done(j){return until(()=>api('/api/jobs/'+j.id),v=>['failed','succeeded'].includes(v.status),'job');}
 launch();
 try{
  await ready();const p=await api('/api/projects','POST',{title:'Longform · prova isolata',aspect_ratio:'16:9'},201),prefix='/api/projects/'+p.id,v1='/api/v1/projects/'+p.id;
  assert.equal((await done(await api(prefix+'/assets','POST',{path:source},202))).status,'succeeded');let project=await api(prefix),a=project.assets[0];
  const raw={language:'it',segments:[{start_ms:0,end_ms:5000,text:'Caffe prima e dopo.'},{start_ms:5000,end_ms:10000,text:'Una seconda frase.'}],words:[{start_ms:500,end_ms:1000,text:'Caffe'},{start_ms:2000,end_ms:4000,text:'prima'},{start_ms:6000,end_ms:7000,text:'dopo.'}]};
  await api(prefix+'/transcripts/import','POST',{asset_id:a.id,transcript:raw});
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1366,height:768}});page.setDefaultTimeout(20000);const errors=[];page.on('pageerror',e=>errors.push(e.message));page.on('dialog',d=>d.accept());
  await page.goto(base);await page.locator('#projects').selectOption(p.id);await navigate(page,'longform');await page.locator('[data-lf-word]').first().waitFor();
  await page.locator('[data-lf-word]').nth(0).click();await page.locator('#lf-correction').fill('Caffè');await page.locator('#lf-correct').click();await page.locator('#lf-save').click();
  await until(()=>api(v1+'/longform/'+a.id),v=>v.words[0].corrected_text==='Caffè','correction persisted');project=await api(prefix);assert.equal(project.transcripts[0].words[0].text,'Caffe','raw ASR stays immutable');
  await until(()=>page.locator('#lf-propose').isEnabled());await page.locator('[data-lf-word]').nth(1).click();await page.locator('[data-lf-action="exclude"]').click();await page.locator('#lf-propose').click();await until(()=>page.locator('#lf-apply').isEnabled());assert.match(await page.locator('#lf-proposal').innerText(),/0:08\.00/);
  await page.locator('#lf-apply').click();await until(()=>api(prefix),v=>v.timeline.clips.length===2,'audio-only rough cut');project=await api(prefix);assert.deepEqual(project.timeline.clips.map(c=>[c.start_ms,c.trim_in_ms,c.duration_ms]),[[0,0,2000],[2000,4000,6000]]);assert(project.timeline.clips.every(c=>project.timeline.tracks.find(t=>t.id===c.track_id).kind==='audio'));
  const beforePreview=project.updated_at,docPreview=await api(v1+'/longform/'+a.id),previewJob=await api(v1+'/longform/preview-render','POST',{asset_id:a.id,expected_updated_at:docPreview.project_updated_at,expected_revision:docPreview.state.revision,at_source_ms:2000},202),previewDone=await done(previewJob);assert.equal(previewDone.status,'succeeded',previewDone.message);assert.equal((await api(prefix)).updated_at,beforePreview,'A/B render must not apply the proposed cuts again');assert.equal(previewDone.result.duration_ms,8000);const previewResponse=await fetch(base+v1+'/longform/previews/'+previewDone.result.preview_file,{headers:{Range:'bytes=0-127'}});assert.equal(previewResponse.status,206);assert.equal((await previewResponse.arrayBuffer()).byteLength,128);
  // Same proposal applied twice is idempotent, not appended as another take.
  let doc=await api(v1+'/longform/'+a.id);await api(v1+'/longform/apply','POST',{asset_id:a.id,expected_updated_at:doc.project_updated_at,expected_revision:doc.state.revision});assert.equal((await api(prefix)).timeline.clips.length,2);
  const mapping=await api(v1+'/time-map','POST',{asset_id:a.id,range:{start_ms:6000,end_ms:7000}});assert.deepEqual(mapping.mapping[0].timeline,{start_ms:4000,end_ms:5000});
  const cues=await api(v1+'/captions/proposal');assert(cues.cues.some(c=>c.text.includes('Caffè')));assert(cues.cues.some(c=>c.word_ids.length));assert(cues.cues.every(c=>c.timeline.end_ms<=8000));assert(!cues.cues.some(c=>c.text.includes('prima')),'excluded word must not reappear in timeline captions');
  const srt=await(await fetch(base+v1+'/captions/sidecar/srt')).text(),vtt=await(await fetch(base+v1+'/captions/sidecar/vtt')).text();assert(srt.includes('Caffè'));assert(vtt.startsWith('WEBVTT\n'));assert(vtt.includes('00:00:04.000 --> 00:00:05.000'));
  doc=await api(v1+'/longform/'+a.id);const stale=structuredClone(doc.state);stale.revision--;await api(v1+'/longform','PUT',{state:stale,expected_updated_at:doc.project_updated_at},409);
  // Restore part of an excluded interval with a later Keep decision.
  const restored=structuredClone(doc.state);restored.decisions.push({id:crypto.randomUUID(),asset_id:a.id,range:{start_ms:2500,end_ms:3000},action:'keep',reason:'Rilancio intenzionale'});project=await api(v1+'/longform','PUT',{state:restored,expected_updated_at:doc.project_updated_at});
  const proposed=await api(v1+'/longform/preview','POST',{asset_id:a.id,expected_updated_at:project.updated_at,expected_revision:project.longform.revision});assert.equal(proposed.mapping.reduce((n,m)=>n+m.source.end_ms-m.source.start_ms,0),8500);
  const exported=await done(await api(prefix+'/export','POST',undefined,202));assert.equal(exported.status,'succeeded',exported.message);const output=exported.output_paths.find(v=>v.endsWith('.mp4'));const probed=JSON.parse(execFileSync(path.join(root,'app/tools/ffprobe.exe'),['-v','error','-show_format','-show_streams','-of','json',output]));assert(Math.abs(Number(probed.format.duration)-8)<.15);assert(probed.streams.some(s=>s.codec_type==='audio'));assert(probed.streams.some(s=>s.codec_type==='video'));
  // Server restart retains corrections and unapproved decisions.
  server.kill();await new Promise(r=>server.once('exit',r));launch();await ready();doc=await api(v1+'/longform/'+a.id);assert.equal(doc.words[0].corrected_text,'Caffè');assert.equal(doc.state.decisions.length,2);
  await page.reload();await page.locator('#projects').selectOption(p.id);await navigate(page,'longform');await page.locator('[data-lf-word]').first().waitFor();assert.match(await page.locator('[data-lf-word]').nth(0).innerText(),/Caffè/);
  await page.screenshot({path:path.join(__dirname,'artifacts/longform-desktop.png'),fullPage:true});await page.setViewportSize({width:390,height:844});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+2));assert.deepEqual(errors,[]);
  console.log('PASS Longform: raw/corrected preservation, source/timeline mapping, exclude/keep review, audio-only timeline, idempotent apply, stale revision rejection, real MP4 export, persistence after server restart, mobile. Imported synthetic ASR; no inference.');
 }catch(e){if(page)await page.screenshot({path:path.join(__dirname,'artifacts/longform-failure.png'),fullPage:true}).catch(()=>{});console.error(logs);throw e;}
 finally{await browser?.close();if(server?.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}if(path.dirname(dir)===fs.realpathSync(os.tmpdir())&&path.basename(dir).startsWith('nl-longform-'))fs.rmSync(dir,{recursive:true,force:true});}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
