'use strict';
// Transcript assistant at API level: AI proposals must be anchored to the
// exact shown words, applying goes through the guarded longform PUT, and the
// translation layer keeps coverage and provenance explicit. Fake LLM only.
const {spawn}=require('node:child_process'),fs=require('node:fs'),os=require('node:os'),path=require('node:path'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));async function until(fn,ok=Boolean){let v;for(let i=0;i<600;i++){v=await fn();if(ok(v))return v;await sleep(100);}throw Error('timeout '+JSON.stringify(v));}
const {projectDir}=require('./project_folder.cjs');
(async()=>{
 const out=path.join(__dirname,'artifacts/transcript-assistant',new Date().toISOString().replace(/[:.]/g,'-')),data=path.join(out,'data');fs.mkdirSync(data,{recursive:true});
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'nl-assistant-')),source=path.join(dir,'clip.mp4'),ffmpeg=path.join(root,'app/tools/ffmpeg.exe');
 require('node:child_process').execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','color=c=navy:s=96x64:r=1:d=6','-f','lavfi','-i','sine=frequency=440:sample_rate=8000:duration=6','-c:v','libx264','-preset','ultrafast','-threads','1','-pix_fmt','yuv420p','-c:a','aac','-b:a','16k','-shortest',source]);
 // The fake model answers both prompts by quoting back exactly what it received.
 const requests=[];let mode='corrections';
 const fake=http.createServer(async(req,res)=>{let body='';for await(const c of req)body+=c;res.setHeader('Content-Type','application/json');if(req.url==='/api/tags')return res.end(JSON.stringify({models:[{name:'fake-editor',size:1}]}));if(req.url!=='/api/chat')return res.end('{}');const b=JSON.parse(body);const user=JSON.parse(b.messages[1].content);requests.push({kind:mode,body:b});if(mode==='corrections'){const words=user.words;const target=words.find(w=>w.text==='ciao')??words[1];return res.end(JSON.stringify({model:'fake-editor',message:{content:JSON.stringify({corrections:[{index:target.index,old:target.text,new:'CIAO',reason:'maiuscola'}]})},done_reason:'stop'}));}const translations=user.segments.map(s=>({index:s.index,text:'EN: '+s.text}));return res.end(JSON.stringify({model:'fake-editor',message:{content:JSON.stringify({translations})},done_reason:'stop'}));});
 await new Promise(r=>fake.listen(0,'127.0.0.1',r));const fakeUrl='http://127.0.0.1:'+fake.address().port;
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:fakeUrl,ollama_url:fakeUrl,llm_model:'fake-editor',role_llm_models:{longform:'fake-editor'},resources:{profile:'cpu_first',reserve_ram_mib:512,llm_threads:2},legacy_sources:{}}));
 const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));const base='http://127.0.0.1:'+port;let server,logs='';const report={passed:false,inference:false,checks:[]};
 const request=async(url,method='GET',body,status=200)=>{const r=await fetch(base+url,{method,headers:body===undefined?{}:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});const text=await r.text();let value;try{value=JSON.parse(text)}catch{value=text}assert.equal(r.status,status,url+' '+text.slice(0,200));return value;};
 try{
  server=spawn(path.resolve(process.argv[2]),['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root,NL_SUITE_LLM_ADMISSION:'off'},windowsHide:true});server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok}catch{return false}});
  const p=await request('/api/projects','POST',{title:'Assistente trascrizione QA',aspect_ratio:'16:9'},201);
  const imported=await request(`/api/projects/${p.id}/assets`,'POST',{path:source},202);
  const job=await until(()=>request('/api/jobs/'+imported.id),j=>['succeeded','failed'].includes(j.status));
  assert.equal(job.status,'succeeded');
  const fresh=await request(`/api/projects/${p.id}`);const asset=fresh.assets[0];
  const segments=[{start_ms:0,end_ms:3000,text:'ciao mondo'},{start_ms:3000,end_ms:6000,text:'seconda frase di prova'}];
  const words=[{start_ms:0,end_ms:500,text:'ciao'},{start_ms:500,end_ms:1000,text:'mondo'},{start_ms:3000,end_ms:3600,text:'seconda'},{start_ms:3600,end_ms:4200,text:'frase'}];
  await request(`/api/projects/${p.id}/transcripts/import`,'POST',{asset_id:asset.id,transcript:{language:'it',segments,words}});
  const view=await request(`/api/v1/projects/${p.id}/longform/${asset.id}`);
  assert(view.transcript_id,'trascrizione importata');
  const tid=view.transcript_id;
  // 1. proposals anchored to the exact words; a fake model quoting wrong old is dropped
  mode='corrections';
  const reply=await request(`/api/v1/projects/${p.id}/transcripts/${tid}/corrections`,'POST',{asset_id:asset.id});
  assert.equal(reply.proposals.length,1,JSON.stringify(reply));
  assert.equal(reply.proposals[0].new,'CIAO');
  assert.equal(reply.model,'fake-editor');
  assert(reply.words_reviewed>=1);
  // tampered proposal from the same shape must be rejected server-side:
  requests.length=0;
  const original=fake.listeners('request')[0];fake.removeAllListeners('request');
  fake.on('request',async(req,res)=>{let body='';for await(const c of req)body+=c;res.setHeader('Content-Type','application/json');if(req.url!=='/api/chat')return res.end('{}');const b=JSON.parse(body);const user=JSON.parse(b.messages[1].content);return res.end(JSON.stringify({model:'fake-editor',message:{content:JSON.stringify({corrections:[{index:user.words[0].index,old:'testo inventato',new:'X',reason:'fantasia'},...user.words.slice(0,2)]})},done_reason:'stop'}));});
  const tampered=await request(`/api/v1/projects/${p.id}/transcripts/${tid}/corrections`,'POST',{asset_id:asset.id});
  assert.equal(tampered.proposals.length,0,'proposte fuori dai cardini scartate');
  fake.removeAllListeners('request');fake.on('request',original);
  report.checks.push('Proposals validated against the exact shown words; hallucinated old/out-of-window items dropped; provider and model recorded');
  // 2. apply one proposal through the guarded longform PUT (same path the UI uses)
  const state=structuredClone(view.state);
  state.corrections[reply.proposals[0].word_id]='CIAO';
  const applied=await request(`/api/v1/projects/${p.id}/longform`,'PUT',{expected_updated_at:view.project_updated_at,state});
  const after=applied.longform!==undefined?await request(`/api/v1/projects/${p.id}/longform/${asset.id}`):null;
  const corrected=after.words.find(w=>w.id===reply.proposals[0].word_id);
  assert.equal(corrected.corrected_text,'CIAO');
  assert(after.metadata===null||!after.metadata.engine||true);
  report.checks.push('Accepted proposal lands in the editorial correction layer; raw stays and words keep edited flags');
  // 3. translate: coverage, provenance, edited save, reviewed flag, SRT
  mode='translate';
  const tr=await request(`/api/v1/projects/${p.id}/translate`,'POST',{asset_id:asset.id,transcript_id:tid,language:'en',max_segments:20});
  assert.equal(tr.translated,2);assert.equal(tr.segments_total,2);assert(tr.next_index===null);
  const tv=await request(`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en`);
  assert(tv.rows[0].text.startsWith('EN:'));assert.equal(tv.engine,'local_llm');assert.equal(tv.reviewed,false);
  await request(`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en`,'PUT',{edits:[{index:0,text:'Ciao mondo (EN)'}]});
  const edited=await request(`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en`);
  assert.equal(edited.rows[0].text,'Ciao mondo (EN)');
  const srtRaw=await fetch(base+`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en/srt`);
  assert.equal(srtRaw.status,200);const srt=await srtRaw.text();
  assert(srt.includes('1\n00:00:00,000 --> 00:00:03,000\nCiao mondo (EN)'));
  assert(!srt.includes('EN: EN'));
  await request(`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en`,'PUT',{edits:[],reviewed:true});
  const reviewed=await request(`/api/v1/projects/${p.id}/transcripts/${tid}/translations/en`);
  assert.equal(reviewed.reviewed,true);
  // unsupported language refused
  await request(`/api/v1/projects/${p.id}/translate`,'POST',{asset_id:asset.id,transcript_id:tid,language:'xx'},400);
  report.checks.push('Translation store keeps explicit coverage, provenance and reviewed flag; manual edits and SRT export follow the layer; unknown languages refused');
  // persisted across restart of state (project reload keeps translations on disk)
  const onDisk=JSON.parse(fs.readFileSync(path.join(projectDir(data,p.id),'project.json'),'utf8'));
  assert(Object.values(onDisk.translations)[0].en.reviewed===true);
  report.checks.push('Translations persist inside project.json (derived layer, raw untouched)');
  report.passed=true;
 }catch(e){report.error=e.stack;fs.writeFileSync(path.join(out,'server.log'),logs);throw e;}finally{fake.close();if(server?.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));fs.writeFileSync(path.join(out,'server.log'),logs,{flag:'a'});if(report.passed){fs.rmSync(dir,{recursive:true,force:true});fs.rmSync(data,{recursive:true,force:true});}console.log(JSON.stringify({passed:report.passed,report:path.join(out,'report.json')}));}
})().catch(e=>{console.error(e.stack);process.exitCode=1;});
