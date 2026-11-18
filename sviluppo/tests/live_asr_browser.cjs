const {navigate}=require('./navigation.cjs');
'use strict';
// Opt-in REAL Qwen CPU inference through browser -> backend -> timeline -> MP4.
const {chromium}=require('playwright'),{spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),net=require('node:net');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,test=Boolean,label='wait',ms=90000){let v;const end=Date.now()+ms;while(Date.now()<end){v=await fn();if(test(v))return v;await sleep(250);}throw Error(label+': '+JSON.stringify(v));}
async function main(){
 assert(process.argv.includes('--real-qwen'),'Use --real-qwen for local CPU inference');
 const source=path.join(root,'sviluppo/tests/artifacts/v1-qwen-probe/speech-it.wav');assert(fs.existsSync(source),'Run the read-only M0 voice extract first');
 const sourceHash=crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex');
 const dir=path.join(root,'sviluppo/tests/artifacts','v1-live-asr-'+crypto.randomUUID().slice(0,8)),data=path.join(dir,'data');fs.mkdirSync(data,{recursive:true});fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:'http://127.0.0.1:9',ollama_url:'http://127.0.0.1:9',legacy_sources:{}}));
 const probe=net.createServer();await new Promise(r=>probe.listen(0,'127.0.0.1',r));const port=probe.address().port;await new Promise(r=>probe.close(r));const base='http://127.0.0.1:'+port;
 const server=spawn(path.join(root,'sviluppo/target/release/nl-video-suite.exe'),['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});let logs='',browser,page;server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
 const report={real_inference:true,provider:'qwen3_asr_cpp',device:'CPU',authentic_speech_seconds:8,source_hash:sourceHash,cloud:false,downloads:false,passed:false,stages:[]};
 const api=async(t,m='GET',b,status=200)=>{const r=await fetch(base+t,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined});const v=await r.json();assert.equal(r.status,status,JSON.stringify(v));return v;};
 try{
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  const p=await api('/api/projects','POST',{title:'Test reale Qwen · testo → montaggio → MP4',aspect_ratio:'16:9'},201),prefix='/api/projects/'+p.id;report.project_id=p.id;
  const imported=await api(prefix+'/assets','POST',{path:source},202);await until(()=>api('/api/jobs/'+imported.id),j=>j.status==='succeeded','import');const owned=(await api(prefix)).assets[0];
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1920,height:1080}});const errors=[];page.on('pageerror',e=>errors.push(e.message));page.setDefaultTimeout(30000);
  await page.goto(base);await page.locator('#projects').selectOption(p.id);await navigate(page,'longform');await page.locator('#lf-runner').evaluate(e=>e.closest('details').open=true);await page.locator('#lf-discover').click();await until(()=>page.locator('#lf-model').inputValue(),v=>v.endsWith('.gguf'));await page.locator('#lf-save-asr').click();await page.locator('#lf-engine').selectOption('qwen_gguf');
  const started=Date.now();await page.locator('#lf-transcribe').click();
  const ready=await until(async()=>{const jobs=await api('/api/jobs');for(const j of jobs.filter(j=>j.project_id===p.id&&j.id!==imported.id)){if(!report.stages.includes(j.message))report.stages.push(j.message);assert.notEqual(j.status,'failed',j.message);}return api(prefix);},p=>p.transcripts.some(t=>t.asset_id===owned.id),'real Qwen transcript',180000);
  report.wall_seconds=(Date.now()-started)/1000;const transcript=ready.transcripts.find(t=>t.asset_id===owned.id);report.word_count=transcript.words.length;report.metadata=ready.transcript_metadata[transcript.id];assert(transcript.words.length>=10);assert.equal(report.metadata.engine,'qwen3_asr_cpp');assert.equal(report.metadata.alignment_completed,true);
  await page.locator('[data-lf-word]').first().waitFor();await page.locator('[data-lf-word]').nth(1).click();const raw=await page.locator('#lf-correction').inputValue();await page.locator('#lf-correction').fill(raw.toUpperCase());await page.locator('#lf-correct').click();await page.locator('#lf-save').click();await until(()=>page.locator('#lf-propose').isEnabled());
  await page.locator('#lf-propose').click();await until(()=>page.locator('#lf-apply').isEnabled());await page.locator('#lf-apply').click();await until(()=>api(prefix),p=>p.timeline.clips.length===1,'real ASR timeline');
  await until(()=>page.locator('#lf-export').isEnabled());await page.locator('#lf-export').click();const exported=await until(()=>api('/api/jobs'),jobs=>jobs.some(j=>j.project_id===p.id&&j.kind==='export_timeline'&&j.status==='succeeded'),'real export');const job=exported.find(j=>j.project_id===p.id&&j.kind==='export_timeline'&&j.status==='succeeded');report.output=job.output_paths.find(v=>v.endsWith('.mp4'));assert(report.output);
  const probed=JSON.parse(execFileSync(path.join(root,'app/tools/ffprobe.exe'),['-v','error','-show_format','-show_streams','-of','json',report.output]));assert(Math.abs(Number(probed.format.duration)-8)<.15);report.export_duration=Number(probed.format.duration);
  // Warm-cache rerun must not change word identity or invalidate corrections.
  const cached=await api('/api/v1/projects/'+p.id+'/asr','POST',{asset_id:owned.id,engine:'qwen_gguf',language:'it'},202);const done=await until(()=>api('/api/jobs/'+cached.id),j=>['succeeded','failed'].includes(j.status),'warm cache');assert.equal(done.status,'succeeded',done.message);assert.equal(done.result.cache_hit,true);const again=await api(prefix);assert.equal(again.transcripts[0].id,transcript.id);assert.equal(Object.keys(again.longform.corrections).length,1);report.cache_hit_verified=true;
  assert.equal(crypto.createHash('sha256').update(fs.readFileSync(source)).digest('hex'),sourceHash);await page.screenshot({path:path.join(dir,'longform-real-qwen.png'),fullPage:true});assert.deepEqual(errors,[]);report.passed=true;
  console.log('PASS REAL CPU Qwen browser -> '+report.word_count+' aligned words -> correction -> common timeline -> MP4; warm-cache identity preserved. '+dir);
 }catch(e){report.error=e.stack||String(e);if(page)await page.screenshot({path:path.join(dir,'failure.png'),fullPage:true}).catch(()=>{});throw e;}
 finally{fs.writeFileSync(path.join(dir,'report.json'),JSON.stringify(report,null,2));fs.writeFileSync(path.join(dir,'server.log'),logs);await browser?.close();if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
