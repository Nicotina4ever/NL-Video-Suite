const {navigate}=require('./navigation.cjs');
'use strict';
// Real browser, upload, frame extraction and persistence; synthetic media/fake AI.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),http=require('node:http'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,test=Boolean,label='condition',ms=30000){const end=Date.now()+ms;let v;while(Date.now()<end){v=await fn();if(test(v))return v;await delay(100);}throw Error(label+': '+JSON.stringify(v));}
async function main(){
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'nl-library-')),data=path.join(dir,'data'),workflows=path.join(dir,'workflows');fs.mkdirSync(data);fs.mkdirSync(workflows);
 const original=path.join(dir,'reference.png'),audio=path.join(dir,'voice.wav'),video=path.join(dir,'source.mp4'),ffmpeg=path.join(root,'app/tools/ffmpeg.exe');
 execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','color=c=violet:s=320x180','-frames:v','1',original]);
 execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','sine=frequency=440:duration=1',audio]);
 execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','color=c=blue:s=320x180:r=24:d=1','-c:v','libx264','-pix_fmt','yuv420p',video]);
 const source=fs.readFileSync(original),requests=[],prompts=[],uploads=[];let refId='';
 const definitions={CLIPTextEncode:{input:{required:{text:['STRING']}},output:['CONDITIONING']},LoadImage:{input:{required:{image:['STRING',{image_upload:true}]}},output:['IMAGE']},SaveImage:{input:{required:{images:['IMAGE'],filename_prefix:['STRING']}},output_node:true,output:[]}};
 fs.writeFileSync(path.join(workflows,'reference-fixture.json'),JSON.stringify({positive:{class_type:'CLIPTextEncode',_meta:{title:'Positive prompt'},inputs:{text:'OLD'}},negative:{class_type:'CLIPTextEncode',_meta:{title:'Negative'},inputs:{text:'KEEP NEGATIVE'}},reference:{class_type:'LoadImage',inputs:{image:'source.png'}},save:{class_type:'SaveImage',inputs:{images:['reference',0],filename_prefix:'fixture'}}}));
 const fake=http.createServer(async(req,res)=>{try{
  let chunks=[];for await(const c of req)chunks.push(c);const bytes=Buffer.concat(chunks),json=()=>JSON.parse(bytes.toString()),reply=(v)=>{res.setHeader('Content-Type','application/json');res.end(JSON.stringify(v));};
  if(req.url==='/api/tags')return reply({models:[{name:'fixture-text'},{name:'fixture-vision',capabilities:['vision']}]});
  if(req.url==='/object_info')return reply(definitions);
  if(req.url==='/system_stats')return reply({system:{test_double:true},devices:[]});
  if(req.url==='/queue')return reply({queue_running:[],queue_pending:[]});
  if(req.url==='/api/chat'){
   const b=json();requests.push(b);assert.equal(b.options.num_gpu,0);assert.equal(b.keep_alive,0);await delay(500);
   if(b.messages[1].images){assert.equal(b.model,'fixture-vision');const jpg=Buffer.from(b.messages[1].images[0],'base64');assert.equal(jpg.subarray(0,2).toString('hex'),'ffd8');return reply({message:{content:'Osservazione sintetica: fondo viola, senza persone. Nessuna identità dedotta.'}});}
   if(b.format?.properties?.references)return reply({message:{content:JSON.stringify({references:[{kind:'character',name:'Ada',notes:'Proposta narrativa: esploratrice con cappotto verde.',prompt:'Character sheet, one explorer in a green coat, front side back and close-up, neutral background, no text.',source_reference_id:refId},{kind:'location',name:'Faro',notes:'Proposta narrativa: faro sul mare.',prompt:'Empty lighthouse interior, stone walls, soft daylight, no people, no text.',source_reference_id:null}]})}});
   if(b.format?.properties?.scenes){assert.match(b.messages[1].content,/Il faro custodisce una memoria/);assert.match(b.messages[1].content,/fondo viola/);return reply({message:{content:JSON.stringify({title:'Storia dai materiali',scenes:[{title:'Il faro',narration:'Ada entra nel faro.',visual_prompt:'Subject: Ada in the lighthouse. Preserve references, soft light, no text.',duration_ms:1000,reference_ids:[refId]}]})}});}
   return reply({message:{content:'Character sheet, one explorer in a green coat, front side back and close-up, neutral background, no text.'}});
  }
  if(req.url==='/upload/image'){assert(bytes.includes(source),'original reference bytes uploaded intact');uploads.push(bytes.length);return reply({name:'uploaded.png',subfolder:'',type:'input'});}
  if(req.url==='/prompt'){const b=json();assert.equal(b.prompt.negative.inputs.text,'KEEP NEGATIVE');assert.equal(b.prompt.reference.inputs.image,'uploaded.png');prompts.push(b);return reply({prompt_id:'fake-'+prompts.length});}
  if(req.url.startsWith('/history/'))return reply({[req.url.split('/').pop()]:{status:{completed:true,status_str:'success'},outputs:{save:{images:[{filename:'reference-result.png',subfolder:'',type:'output'}]}}}});
  if(req.url.startsWith('/view?')){res.setHeader('Content-Type','image/png');return res.end(source);}
  return reply({});
 }catch(e){res.statusCode=500;res.end(JSON.stringify({error:e.message}));console.error(e);}});
 await new Promise(r=>fake.listen(0,'127.0.0.1',r));const fakeUrl='http://127.0.0.1:'+fake.address().port;
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({ollama_url:fakeUrl,comfy_url:fakeUrl,llm_model:'fixture-text',role_llm_models:{vision:'fixture-vision'},ollama_execution:'cpu',legacy_sources:{},workflow_roots:[workflows]}));
 const probe=net.createServer();await new Promise(r=>probe.listen(0,'127.0.0.1',r));const port=probe.address().port;await new Promise(r=>probe.close(r));const base='http://127.0.0.1:'+port;
 const exe=process.argv[2]||path.join(root,'sviluppo/target/release/nl-video-suite.exe');const server=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});let logs='',browser,page;
 server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
 try{
  const api=async(t,method='GET',body,status=200)=>{const res=await fetch(base+t,{method,headers:body?{'Content-Type':'application/json'}:{},body:body?JSON.stringify(body):undefined});const v=await res.json();assert.equal(res.status,status,JSON.stringify(v));return v;};
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1550,height:1000}});page.setDefaultTimeout(25000);page.on('dialog',d=>d.accept());const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(base);await page.locator('#new-project').click();await page.locator('#title').fill('Reference library synthetic');await page.locator('#create-form button').first().click();await until(()=>page.locator('#projects').inputValue());
  const pid=await page.locator('#projects').inputValue(),prefix='/api/projects/'+pid,narrative=prefix+'/narrative';
  const saved=()=>api(narrative),jobDone=async(job)=>until(()=>api('/api/jobs/'+job.id),j=>['succeeded','failed'].includes(j.status),'job complete',60000);
  await navigate(page,'story');await until(()=>page.locator('#library-upload').isEnabled());
  await page.locator('#library-upload').setInputFiles([original,audio,video]);
  let prod=await until(saved,p=>p.references.length===3,'three owned imports');
  await until(()=>page.locator('#library-save').isEnabled());refId=prod.references[0].id;const audioRef=prod.references[1],videoRef=prod.references[2];
  assert.deepEqual(fs.readFileSync(original),source);assert.equal((await api(prefix)).timeline.clips.length,0);
  await api(prefix+'/transcripts/import','POST',{asset_id:audioRef.source_asset_id,transcript:{segments:[{start:0,end:1,text:'Il faro custodisce una memoria.'}]}});
  await page.locator(`[data-library-select="${refId}"]`).click();
  await page.locator('[data-library-field="notes"]').fill('Usa questo materiale per uno stile illustrato originale.');await page.locator('#library-save').click();await until(()=>page.locator('#library-save').isEnabled());
  await page.locator('[data-library-action="vision"]').click();
  prod=await until(saved,p=>p.references[0].analysis.includes('fondo viola'),'vision analysis');assert(requests.some(r=>r.messages[1].images));
  await until(()=>page.locator('#library-suggest').isEnabled());await page.locator('#library-suggest').click();
  prod=await until(saved,p=>p.references.length===5,'derived character and location');const character=prod.references.find(r=>r.kind==='character'),location=prod.references.find(r=>r.kind==='location');assert.equal(character.source_asset_id,prod.references[0].source_asset_id);assert.equal(location.source_asset_id,null);assert(!character.approved);
  // Prompt creation works without any source media; no source is fabricated.
  await until(()=>page.locator('#library-save').isEnabled());await page.locator(`[data-library-select="${location.id}"]`).click();await page.locator('[data-library-action="prompt"]').click();await until(async()=>{const p=await saved();return p.job_id&&await api('/api/jobs/'+p.job_id);},j=>j.status==='succeeded','source-free prompt');
  await until(()=>page.locator('#library-save').isEnabled());await page.locator(`[data-library-select="${character.id}"]`).click();
  const wf=(await api('/api/workflows')).find(v=>v.name==='reference-fixture');await page.locator('[data-library-workflow]').selectOption(wf.id);
  await page.locator('[data-library-text] option[value="positive.text"]').waitFor({state:'attached'});await page.locator('[data-library-text]').selectOption('positive.text');await page.locator('[data-library-ref="reference.image"]').selectOption(character.source_asset_id);
  await page.locator('[data-library-action="generate"]').click();prod=await until(saved,p=>p.references.find(r=>r.id===character.id).asset_id,'character generated',60000);const generated=prod.references.find(r=>r.id===character.id);assert(!generated.approved);assert.equal(prompts.length,1);assert.equal(uploads.length,1);
  await until(()=>page.locator('#library-save').isEnabled());await page.locator('[data-library-action="preview"]').click();await until(()=>page.locator('#narrative-preview-image').evaluate(e=>e.complete&&e.naturalWidth>0));await until(()=>page.locator('#library-save').isEnabled());await page.locator('[data-library-action="approve"]').click();await until(saved,p=>p.references.find(r=>r.id===character.id).approved);
  // Source review and audio transcript reuse stay in the story tab.
  await until(()=>page.locator('#library-save').isEnabled());await page.locator(`[data-library-select="${audioRef.id}"]`).click();await page.locator('[data-library-action="transcribe"]').click();await until(saved,p=>p.references.find(r=>r.id===audioRef.id).analysis.includes('1 segmenti'));
  await until(()=>page.locator('#library-save').isEnabled());await page.locator('#narrative-scene-count').fill('1');await page.locator('#narrative-premise').fill('');await page.locator('#narrative-plan').click();
  prod=await until(saved,p=>p.scenes.length===1,'story from materials without premise');assert.deepEqual(prod.scenes[0].reference_ids,[refId]);assert.equal(prod.references.length,5);
  // Bad ownership, stale revision and approval forgery must fail or be ignored.
  const bad=structuredClone(prod);bad.references[0].source_asset_id=crypto.randomUUID();await api(narrative,'PUT',{production:bad},400);
  const stale=structuredClone(prod);stale.revision--;await api(narrative,'PUT',{production:stale},409);
  const forged=structuredClone(prod);forged.references[0].approved=true;const safe=await api(narrative,'PUT',{production:forged});assert.equal(safe.references[0].approved,false);
  await api(narrative+'/references/approve','POST',{reference_id:refId,asset_id:generated.asset_id},400);
  // One selected video frame goes to vision, never an invented full video analysis.
  const frameJob=await api(narrative+'/references/analyze','POST',{reference_id:videoRef.id,operation:'vision',at_ms:300},202);assert.equal((await jobDone(frameJob)).status,'succeeded');await api(narrative+'/references/analyze','POST',{reference_id:videoRef.id,operation:'vision',at_ms:2000},400);
  assert.equal(requests.filter(r=>r.messages[1].images).length,2);
  await page.reload();await page.locator('#projects').selectOption(pid);await navigate(page,'story');await until(()=>page.locator('#library-list button').count(),n=>n===5,'persisted library');
  await page.locator(`[data-library-select="${character.id}"]`).click();assert.equal(await page.locator('[data-library-text]').inputValue(),'positive.text');assert.equal(await page.locator('[data-library-ref="reference.image"]').inputValue(),character.source_asset_id);
  await page.screenshot({path:path.join(__dirname,'artifacts/library-desktop.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+2));await page.screenshot({path:path.join(__dirname,'artifacts/library-mobile.png'),fullPage:true});assert.deepEqual(errors,[]);
  console.log('PASS narrative library: multi-upload, owned sources, real JPEG frame -> fake vision, cached Italian transcript, AI character/location proposals, source-free prompt, reference graph upload, preview/review, media-first story, persistence, ownership, stale saves, mobile; no real inference');
 }catch(e){if(page)await page.screenshot({path:path.join(__dirname,'artifacts/library-failure.png'),fullPage:true}).catch(()=>{});console.error(logs);throw e;}
 finally{await browser?.close();server.kill();await new Promise(r=>server.once('exit',r));await new Promise(r=>fake.close(r));if(path.dirname(dir)===fs.realpathSync(os.tmpdir())&&path.basename(dir).startsWith('nl-library-'))fs.rmSync(dir,{recursive:true,force:true});}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
