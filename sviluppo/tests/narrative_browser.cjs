const {navigate}=require('./navigation.cjs');
'use strict';
// Real UI and FFmpeg with synthetic local media; no AI inference.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const assert=require('node:assert/strict'),http=require('node:http'),net=require('node:net');
const root=path.resolve(__dirname,'../..'),artifacts=path.join(__dirname,'artifacts');
const ffmpeg=path.join(root,'app/tools/ffmpeg.exe'),ffprobe=path.join(root,'app/tools/ffprobe.exe');
const delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function until(fn,test,label,timeout=25000){const end=Date.now()+timeout;let value;while(Date.now()<end){value=await fn();if(test(value))return value;await delay(100);}throw Error(label+': '+JSON.stringify(value));}
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function main(){
 const directory=fs.mkdtempSync(path.join(os.tmpdir(),'nl-narrative-ui-'));
 const data=path.join(directory,'data');fs.mkdirSync(data);
 const source=path.join(directory,'visual.png'),voice=path.join(directory,'voce-sintetica.wav');
 const media=args=>execFileSync(ffmpeg,['-nostdin','-v','error','-n',...args],{windowsHide:true});
 media(['-f','lavfi','-i','color=c=0x7531ff:s=320x180:d=0.1','-frames:v','1',source]);
 media(['-f','lavfi','-i','sine=frequency=440:sample_rate=16000:duration=1.3',voice]);
 const definitions={CLIPTextEncode:{input:{required:{text:['STRING']}},output:['CONDITIONING']},UNETLoader:{input:{required:{unet_name:[['fixture-a.safetensors','fixture-b.safetensors']]}},output:['MODEL']}};
 const fake=http.createServer((req,res)=>{res.setHeader('Content-Type','application/json');res.end(JSON.stringify(req.url==='/api/tags'?{models:[]}:req.url==='/object_info'?definitions:{}));});
 await new Promise(r=>fake.listen(0,'127.0.0.1',r));const fakeUrl='http://127.0.0.1:'+fake.address().port;
 const workflows=path.join(directory,'workflows');fs.mkdirSync(workflows);
 fs.writeFileSync(path.join(workflows,'synthetic-controls.json'),JSON.stringify({text:{class_type:'CLIPTextEncode',inputs:{text:'Original prompt'}},model:{class_type:'UNETLoader',inputs:{unet_name:'fixture-a.safetensors'}},save:{class_type:'SaveImage',inputs:{filename_prefix:'fixture'}}}));
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:fakeUrl,ollama_url:fakeUrl,legacy_sources:{},workflow_roots:[workflows]}));
 const port=await freePort(),base='http://127.0.0.1:'+port;
 const executable=process.argv[2]?path.resolve(process.argv[2]):path.join(root,'app/nl-video-suite.exe');
 const server=spawn(executable,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true,stdio:['ignore','pipe','pipe']});
 let logs='',browser,page;const errors=[];server.stdout.on('data',v=>logs+=v);server.stderr.on('data',v=>logs+=v);
 try{
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}},Boolean,'startup');
  browser=await chromium.launch({channel:'msedge',headless:true});
  page=await browser.newPage({viewport:{width:1600,height:1000}});page.setDefaultTimeout(20000);
  page.on('pageerror',e=>errors.push(e.message));
  page.on('dialog',d=>d.accept());
  await page.goto(base);await page.locator('#new-project').click();
  await page.locator('#title').fill('Produzione narrativa · prova sintetica');await page.locator('#create-form button').first().click();
  await page.waitForFunction(()=>document.querySelector('#notice').textContent==='Progetto creato.');
  const pid=await page.locator('#projects').inputValue(),prefix=base+'/api/projects/'+pid;
  assert(!fs.existsSync(path.join(data,'projects',pid,'narrative.json')),'Opening NLE must not create a narrative');
  const get=async tail=>{const r=await page.request.get(prefix+tail);assert(r.ok());return r.json();};
  for(const file of [source,voice]){
   const r=await page.request.post(prefix+'/assets',{data:{path:file}});assert.equal(r.status(),202);
   const job=await r.json();
   await until(async()=>{const r=await page.request.get(base+'/api/jobs/'+job.id);return r.json();},j=>j.status==='succeeded','import');
  }
  await page.waitForFunction(()=>state.project?.assets.length===2);
  const p=await get(''),image=p.assets.find(a=>a.kind==='image'),audio=p.assets.find(a=>a.kind==='audio');
  await navigate(page,'story');await page.locator('#narrative-add').waitFor();
  await until(()=>page.locator('#narrative-add').isEnabled(),Boolean,'production loaded');
  await page.locator('#narrative-add').click();
  let cards=page.locator('[data-narrative-scene]');
  await cards.nth(0).locator('[data-narrative-field="title"]').fill('Voce e visual');
  await cards.nth(0).locator('[data-narrative-field="narration"]').fill('Una prova di voce italiana.');
  await cards.nth(0).locator('[data-narrative-field="visual_prompt"]').fill('Microfono editoriale su fondo viola, luce morbida.');
  await cards.nth(0).locator('[data-narrative-field="duration_ms"]').fill('0.7');
  await cards.nth(0).locator('[data-narrative-field="voice_mode"]').selectOption('manual');
  await cards.nth(0).locator('[data-narrative-asset="voice"]').selectOption(audio.id);
  await cards.nth(0).locator('[data-narrative-asset="visual"]').selectOption(image.id);
  const workflowCatalog=await(await page.request.get(base+'/api/workflows')).json();
  await cards.nth(0).locator('details').filter({has:page.locator('[data-narrative-workflow="visual"]')}).evaluate(e=>e.open=true);
  await cards.nth(0).locator('[data-narrative-workflow="visual"]').selectOption(workflowCatalog.find(w=>w.name==='synthetic-controls').id);
  await cards.nth(0).locator('[data-narrative-binding="visual"] option[value="text.text"]').waitFor({state:'attached'});
  await cards.nth(0).locator('details').filter({has:page.locator('[data-narrative-workflow="visual"]')}).evaluate(e=>e.open=true);
  await cards.nth(0).locator('[data-narrative-binding="visual"]').selectOption('text.text');
  await cards.nth(0).locator('.narrative-models summary').click();
  await cards.nth(0).locator('[data-narrative-scalar="model.unet_name"]').selectOption(JSON.stringify('fixture-b.safetensors'));
  await page.locator('#narrative-add').click();
  await cards.nth(1).locator('[data-narrative-field="narration"]').fill('Conclusione della prova.');
  await cards.nth(1).locator('[data-narrative-field="duration_ms"]').fill('0.8');
  await cards.nth(1).locator('[data-narrative-asset="visual"]').selectOption(image.id);
  await page.locator('#narrative-save').click();
  const saved=await until(()=>get('/narrative'),v=>v.scenes.length===2,'saved scenes');
  assert.equal(saved.scenes[0].visual_workflow.inputs['model.unet_name'],'fixture-b.safetensors','explicit actual-schema model dropdown persisted');
  await until(()=>page.locator('#narrative-save').isEnabled(),Boolean,'save finished');
  // Polling must preserve a new requested plan length even without editing scenes.
  await page.locator('#narrative-scene-count').fill('3');await page.locator('#narrative-duration').fill('9');
  await page.waitForTimeout(2800);
  assert.equal(await page.locator('#narrative-scene-count').inputValue(),'3');
  await page.locator('#narrative-premise').fill('Una nuova idea per il collaudo');
  let planRequest;
  await page.route('**/narrative/plan',route=>{planRequest=route.request().postDataJSON();route.fulfill({status:400,contentType:'application/json',body:JSON.stringify({error:'Richiesta intercettata dal test: nessuna inferenza'})});});
  await page.locator('#narrative-plan').click();
  await until(async()=>planRequest,Boolean,'captured plan');
  assert.equal(planRequest.scene_count,3);assert.equal(planRequest.target_duration_ms,9000);
  await until(()=>page.locator('#narrative-save').isEnabled(),Boolean,'plan action ended');
  await page.unroute('**/narrative/plan');
  await page.locator('#narrative-approve-script').click();
  await until(()=>get('/narrative'),v=>v.script_approved,'script approval');
  await until(()=>page.locator('#narrative-assemble').isEnabled(),Boolean,'review action ended');
  await page.locator('#narrative-assemble').click();
  await page.waitForFunction(()=>document.querySelector('#narrative-error').textContent.includes('approva prima il visual'));
  assert.equal((await get('')).timeline.clips.length,0);
  for(const[index,kind]of [[0,'visual'],[0,'voice'],[1,'visual']]){
   await until(()=>cards.nth(index).locator(`[data-narrative-preview="${kind}"]`).isEnabled(),Boolean,'preview enabled');
   await cards.nth(index).locator(`[data-narrative-preview="${kind}"]`).click();
   await page.locator('#narrative-approve-media').waitFor();
   if(kind==='visual')await page.waitForFunction(()=>document.querySelector('#narrative-preview-image').naturalWidth>0);
   else await page.waitForFunction(()=>document.querySelector('#narrative-preview-audio').readyState>=2);
   await until(()=>page.locator('#narrative-save').isEnabled(),Boolean,'preview action ended');
   await page.locator('#narrative-approve-media').click();
   await until(()=>get('/narrative'),v=>v.scenes[index][kind+'_approved'],'media approval');
   await until(()=>page.locator('#narrative-assemble').isEnabled(),Boolean,'approval action ended');
  }
  // Selection limits generation; assembly deliberately includes the full story.
  await cards.nth(0).locator('[data-narrative-select]').uncheck();
  await page.locator('#narrative-assemble').click();
  let assembled=await until(()=>get(''),v=>v.timeline.clips.length===3,'assembly');
  assert.equal(Math.max(...assembled.timeline.clips.map(c=>c.start_ms+c.duration_ms)),2100);
  await until(()=>page.locator('#narrative-export').isEnabled(),Boolean,'assembly ended');
  await page.locator('#narrative-assemble').click();
  await until(()=>page.locator('#narrative-export').isEnabled(),Boolean,'reassembly ended');
  assert.equal((await get('')).timeline.clips.length,3,'idempotent assembly');
  await page.locator('#narrative-subtitles').check();await page.locator('#narrative-export').click();
  await page.locator('#narrative-download a').first().waitFor({timeout:30000});
  assert.equal(await page.locator('#narrative-download a').count(),2,'video AND scene-timed SRT links');
  const production=await get('/narrative');
  const job=await(await page.request.get(base+'/api/jobs/'+production.export_job_id)).json();
  assert.equal(job.status,'succeeded');
  assert(job.result.scene_timed_subtitles,'scene-timed subtitles metadata must survive encoding result');
  assert(job.result.encoding.actual.streams.some(s=>s.codec_type==='video'),'encoding metadata must coexist with scene subtitle metadata');
  const info=JSON.parse(execFileSync(ffprobe,['-v','error','-show_entries','format=duration','-of','json',job.output_paths[0]],{windowsHide:true}));
  assert(Math.abs(Number(info.format.duration)-2.1)<.1);
  const srt=await page.request.get(base+await page.locator('#narrative-download a').last().getAttribute('href'));
  assert(srt.ok());assert((await srt.text()).includes('00:00:01,300'));
  await page.locator('#narrative-open-studio').click();
  assert(await page.locator('#tab-studio').evaluate(e=>e.classList.contains('active')));
  await until(()=>page.locator('[data-nle-clip]').count(),v=>v===3,'shared NLE timeline');
  await navigate(page,'story');
  await cards.nth(0).locator('[data-narrative-field="title"]').fill('Modifica ancora da salvare');
  await page.waitForTimeout(2800);
  assert.equal(await cards.nth(0).locator('[data-narrative-field="title"]').inputValue(),'Modifica ancora da salvare');
  await page.locator('#narrative-save').click();await until(()=>get('/narrative'),v=>v.scenes[0].title==='Modifica ancora da salvare','dirty draft persisted');
  await page.reload();await navigate(page,'story');
  await page.waitForFunction(()=>document.querySelector('[data-narrative-field="title"]')?.value==='Modifica ancora da salvare');
  fs.mkdirSync(artifacts,{recursive:true});await page.screenshot({path:path.join(artifacts,'narrative-desktop.png'),fullPage:true});
  await page.setViewportSize({width:390,height:844});
  assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'mobile overflow');
  assert.deepEqual(errors,[]);
  fs.writeFileSync(path.join(artifacts,'narrative-report.json'),JSON.stringify({passed:true,synthetic:true,ai_inference:false,checks:['manual scenes','plan duration preserved','script review','media preview and approval','voice timing','all-scene idempotent assembly','real FFmpeg export','SRT download','same NLE timeline','dirty draft survives polling','reload','mobile']},null,2));
  console.log('PASS narrative browser: scenes, review, audio timing, export/SRT, shared NLE, polling/reload/mobile. Synthetic media; no AI inference.');
 }catch(e){fs.mkdirSync(artifacts,{recursive:true});if(page)await page.screenshot({path:path.join(artifacts,'narrative-failure.png'),fullPage:true}).catch(()=>{});fs.writeFileSync(path.join(artifacts,'narrative-failure.log'),logs);throw e;
 }finally{
  if(browser)await browser.close();
  if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}
  await new Promise(r=>fake.close(r));
  assert.equal(path.dirname(directory),path.resolve(os.tmpdir()));assert(path.basename(directory).startsWith('nl-narrative-ui-'));
  fs.rmSync(directory,{recursive:true,force:true,maxRetries:10,retryDelay:100});
 }
}
main().catch(e=>{console.error(e);process.exitCode=1;});
