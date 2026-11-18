const {navigate}=require('./navigation.cjs');
'use strict';
// Explicitly opt-in, authentic 58-minute CPU ASR. Isolated data, one owned copy
// of the input. Incremental report/cache survive interruption; master read-only.
const {chromium}=require('playwright'),{spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),net=require('node:net');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,ok=Boolean,label='wait',ms=120000){let v;const end=Date.now()+ms;while(Date.now()<end){v=await fn();if(ok(v))return v;await sleep(1000);}throw Error(label+': '+JSON.stringify(v));}
async function main(){
 assert(process.argv.includes('--real-qwen'),'Use --real-qwen: actual CPU inference on authentic local speech');
 const source='K:\\AI\\ClipsAI\\video_da_sgrossare\\Gemini2.mp4';assert(fs.existsSync(source));
 const dir=path.join(root,'sviluppo/tests/artifacts/v1-authentic-longform'),data=path.join(dir,'data');fs.mkdirSync(data,{recursive:true});
 const reportPath=path.join(dir,'report.json'),report={real_inference:true,synthetic:false,source,source_stat:{size:fs.statSync(source).size,mtime_ms:fs.statSync(source).mtimeMs},device:'CPU',downloads:false,passed:false,stages:[],started_at:new Date().toISOString()};
 const save=()=>fs.writeFileSync(reportPath,JSON.stringify(report,null,2));save();
 const exe=path.join(dir,'nl-video-suite-test.exe');fs.copyFileSync(path.join(root,'sviluppo/target/release/nl-video-suite.exe'),exe);report.exe_sha256=crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex');
 if(!fs.existsSync(path.join(data,'settings.json')))fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:'http://127.0.0.1:9',ollama_url:'http://127.0.0.1:9',legacy_sources:{}}));
 const probe=net.createServer();await new Promise(r=>probe.listen(0,'127.0.0.1',r));const port=probe.address().port;await new Promise(r=>probe.close(r));const base='http://127.0.0.1:'+port;
 const server=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});let logs='',browser,page;server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);report.port=port;
 const api=async(t,m='GET',b,status=200)=>{const r=await fetch(base+t,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined});const v=await r.json();assert.equal(r.status,status,JSON.stringify(v));return v;};
 try{
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  let p=(await api('/api/projects')).find(p=>p.title==='M0–M8 · Gemini2 autentico, isolato');if(!p)p=await api('/api/projects','POST',{title:'M0–M8 · Gemini2 autentico, isolato',aspect_ratio:'16:9'},201);const prefix='/api/projects/'+p.id;report.project_id=p.id;save();
  p=await api(prefix);if(!p.assets.length){const j=await api(prefix+'/assets','POST',{path:source},202);const imported=await until(()=>api('/api/jobs/'+j.id),j=>['failed','succeeded'].includes(j.status),'import',300000);assert.equal(imported.status,'succeeded',imported.message);p=await api(prefix);}const asset=p.assets.find(a=>a.file_name==='Gemini2.mp4');assert(asset);report.asset_id=asset.id;report.authentic_duration_ms=asset.media.duration_ms;save();
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(60000);const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base);await page.locator('#projects').selectOption(p.id);await navigate(page,'longform');await page.locator('#lf-runner').evaluate(e=>e.closest('details').open=true);await page.locator('#lf-discover').click();await until(()=>page.locator('#lf-model').inputValue(),v=>v.endsWith('.gguf'));await page.locator('#lf-threads').fill('4');await page.locator('#lf-save-asr').click();await page.locator('#lf-engine').selectOption('qwen_gguf');await page.locator('#lf-transcribe').click();
  let lastMessage='',lastLog=0;const start=Date.now();const ready=await until(async()=>{const jobs=(await api('/api/jobs')).filter(j=>j.project_id===p.id);const failed=jobs.find(j=>j.status==='failed'&&Date.parse(j.created_at)>=start-10000);assert(!failed,failed?.message);const running=jobs.find(j=>['running','queued'].includes(j.status));if(running&&running.message!==lastMessage){lastMessage=running.message;report.stages.push({at:new Date().toISOString(),message:lastMessage});save();if(Date.now()-lastLog>20000){console.log(lastMessage);lastLog=Date.now();}}return api(prefix);},p=>p.transcripts.some(t=>t.asset_id===asset.id),'authentic longform ASR',7200000);
  report.wall_seconds=(Date.now()-start)/1000;const t=ready.transcripts.find(t=>t.asset_id===asset.id);report.word_count=t.words.length;report.segment_count=t.segments.length;report.metadata=ready.transcript_metadata[t.id];assert(t.words.length>1000);assert(t.words.at(-1).end_ms>3000000,'must transcribe the end, not only an extract');assert(t.words.every(w=>w.end_ms>w.start_ms&&w.start_ms>=0&&w.end_ms<=asset.media.duration_ms+100));report.rtf=report.wall_seconds/(asset.media.duration_ms/1000);
  await page.locator('[data-lf-word]').first().waitFor();assert(await page.locator('[data-lf-word]').count()<=200);await page.locator('#lf-next').click();await until(()=>page.locator('#lf-page').innerText(),v=>v.startsWith('201'));await page.screenshot({path:path.join(dir,'longform-authentic.png'),fullPage:true});assert.deepEqual(errors,[]);
  assert.equal(fs.statSync(source).size,report.source_stat.size);assert.equal(fs.statSync(source).mtimeMs,report.source_stat.mtime_ms);report.master_stat_unchanged=true;report.passed=true;report.completed_at=new Date().toISOString();console.log('PASS REAL authentic longform Qwen CPU: '+report.word_count+' words, '+report.wall_seconds+'s. '+reportPath);
 }catch(e){report.error=e.stack||String(e);throw e;}
 finally{save();fs.writeFileSync(path.join(dir,'server.log'),logs);await browser?.close();if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
