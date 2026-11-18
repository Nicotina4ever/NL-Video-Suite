const {navigate}=require('./navigation.cjs');
'use strict';
// Authentic existing test project only. No copy/import, ASR or LLM inference.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),dir=path.join(__dirname,'artifacts/v1-authentic-longform'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,p=Boolean,ms=30000){const end=Date.now()+ms;let v;while(Date.now()<end){v=await fn();if(p(v))return v;await sleep(500);}throw Error('Timeout: '+JSON.stringify(v));}
async function hash(file){const h=crypto.createHash('sha256');for await(const b of fs.createReadStream(file))h.update(b);return h.digest('hex');}
async function main(){
 assert(process.argv.includes('--authentic'),'Explicit authentic test required');
 const asr=JSON.parse(fs.readFileSync(path.join(dir,'report.json'))),editing=JSON.parse(fs.readFileSync(path.join(dir,'editing-resume-report.json')));assert(asr.passed&&editing.passed);
 const data=path.join(dir,'data'),file=path.join(data,'projects',asr.project_id,'project.json'),fixture=JSON.parse(fs.readFileSync(file));
 // This is a disposable test project. Change its output ratio, never a personal project.
 assert.equal(fixture.id,asr.project_id);assert(file.startsWith(path.join(root,'sviluppo/tests/artifacts/')));
 fixture.aspect_ratio='16:9';fixture.timeline.clips=fixture.timeline.clips.filter(c=>fixture.clip_origins[c.id]?.producer==='longform');
 assert(fixture.timeline.clips.length);fs.writeFileSync(file,JSON.stringify(fixture,null,2));
 const report={passed:false,real_media:true,new_ai_inference:false,asr_report:'report.json',topics_report:'editing-report.json',shorts_report:'editing-resume-report.json',render_wall_seconds:0,system_free_ram_min_bytes:os.freemem()},out=path.join(dir,'longform-delivery-report.json');
 const save=()=>fs.writeFileSync(out,JSON.stringify(report,null,2));save();
 const probe=net.createServer();await new Promise(r=>probe.listen(0,'127.0.0.1',r));const port=probe.address().port;await new Promise(r=>probe.close(r));
 const exe=path.join(dir,'suite-delivery-test.exe');fs.copyFileSync(path.join(root,'sviluppo/target/release/nl-video-suite.exe'),exe);
 const server=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true}),base='http://127.0.0.1:'+port,prefix='/api/projects/'+fixture.id,v1='/api/v1/projects/'+fixture.id;let browser,page,logs='';server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
 const api=async(t,m='GET',b)=>{const r=await fetch(base+t,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined});const v=await r.json();assert(r.ok,JSON.stringify(v));return v;};
 try{
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  const source='K:\\AI\\ClipsAI\\video_da_sgrossare\\Gemini2.mp4',before=fs.statSync(source),raw=JSON.stringify(fixture.transcripts);report.master_sha256=await hash(source);assert.equal(report.master_sha256,asr.source_sha256||'40f8e4faaa8b7f1ff016fd7fb380bc9e02d8837652b6435c7c41763a0b9f8e28');
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1600,height:1000}});page.on('dialog',d=>d.accept());const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(base);await page.locator('#projects').selectOption(fixture.id);await navigate(page,'longform');await page.locator('[data-lf-word]').first().waitFor();
  await page.locator('[data-lf-word]').first().click();await page.locator('#lf-correction').fill('Allora,');await page.locator('#lf-correct').click();await page.locator('#lf-save').click();await until(()=>page.locator('#lf-propose').isEnabled());
  await page.locator('#lf-suggest-cuts').click();let p=await until(()=>api(prefix),p=>p.longform.cut_candidates.length>0);report.cut_proposal_count=p.longform.cut_candidates.length;report.cut_kind_counts=p.longform.cut_candidates.reduce((a,c)=>(a[c.kind]=(a[c.kind]||0)+1,a),{});report.cut_samples=p.longform.cut_candidates.slice(0,3);assert(p.longform.cut_candidates.every(c=>!c.approved),'Suggestions require review, no automatic deletion');
  await until(()=>page.locator('#lf-propose').isEnabled());await page.locator('#lf-propose').click();await until(()=>page.locator('#lf-apply').isEnabled());await page.locator('#lf-apply').click();await until(()=>page.locator('#lf-export').isEnabled());
  const response=page.waitForResponse(r=>r.url()===base+prefix+'/export'&&r.request().method()==='POST');await page.locator('#lf-export').click();const j=await(await response).json(),started=Date.now();report.job_id=j.id;report.port=port;save();
  const completed=await until(async()=>{const current=await api('/api/jobs/'+j.id);report.render_wall_seconds=(Date.now()-started)/1000;report.system_free_ram_min_bytes=Math.min(report.system_free_ram_min_bytes,os.freemem());report.status=current.status;save();return current;},j=>['failed','succeeded'].includes(j.status),7200000);assert.equal(completed.status,'succeeded',completed.message);
  const output=completed.output_paths.find(p=>p.endsWith('.mp4')),info=JSON.parse(execFileSync(path.join(root,'app/tools/ffprobe.exe'),['-v','error','-show_format','-show_streams','-of','json',output]));report.output=output;report.output_bytes=fs.statSync(output).size;report.duration_seconds=Number(info.format.duration);const video=info.streams.find(s=>s.codec_type==='video'),audio=info.streams.find(s=>s.codec_type==='audio');assert.equal(video.width,1920);assert.equal(video.height,1080);assert(audio);assert(Math.abs(report.duration_seconds-asr.authentic_duration_ms/1000)<.1);
  report.chapters=await api(v1+'/longform/chapters/json');assert(report.chapters.youtube_valid);fs.writeFileSync(path.join(dir,'longform-final-chapters.json'),JSON.stringify(report.chapters,null,2));
  p=await api(prefix);assert.equal(JSON.stringify(p.transcripts),raw);assert.equal(p.longform.corrections[Object.keys(p.longform.corrections)[0]],'Allora,');assert.equal(fs.statSync(source).size,before.size);assert.equal(fs.statSync(source).mtimeMs,before.mtimeMs);assert.deepEqual(errors,[]);report.human_editorial_approval=false;report.creative_quality_certified=false;report.passed=true;save();console.log('PASS authentic 58-minute MP4 UI export, correction, unapproved cut suggestions and mapped chapters. '+out);
 }catch(e){report.error=e.stack;throw e;}finally{save();fs.writeFileSync(path.join(dir,'delivery-server.log'),logs);await browser?.close();if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
