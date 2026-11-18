const {navigate}=require('./navigation.cjs');
// Actual Rust HTTP/browser regression with synthetic Ollama/compatible servers.
// Never starts, stops, modifies or downloads real providers or models.
const {chromium}=require('playwright');
const {spawn}=require('node:child_process');
const fs=require('node:fs');
const os=require('node:os');
const path=require('node:path');
const net=require('node:net');
const http=require('node:http');
const assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..');
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
async function freePort(){const s=net.createServer();await new Promise(r=>s.listen(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;}
async function main(){
 const fixture=fs.mkdtempSync(path.join(os.tmpdir(),'nl-provider-profiles-test-'));
 const requests=[];
 const makeFake=kind=>http.createServer(async(req,res)=>{
  const chunks=[];for await(const chunk of req)chunks.push(chunk);
  const body=chunks.length?JSON.parse(Buffer.concat(chunks)):null;
  requests.push({kind,path:req.url,method:req.method,body});
  let value,status=200;
  if(req.url==='/system_stats')value={devices:[]};
  else if(kind==='ollama'&&req.url==='/api/tags')value={models:[{name:'writer:latest'},{name:'graphics:latest'}]};
  else if(kind==='compatible'&&req.url==='/v1/models')value={data:[{id:'Local Author Q4.gguf'},{id:'Local Director Q4.gguf'}]};
  else if(kind==='compatible'&&req.url==='/v1/chat/completions')value={model:body.model,choices:[{finish_reason:'stop',message:{content:JSON.stringify({premise:'Fixture sintetica',characters:[],locations:[],scenes:[{title:'Solo test',description:'Nessuna generazione reale'}]})}}]};
  else {status=404;value={error:'Not a simulated endpoint'};}
  res.writeHead(status,{'Content-Type':'application/json'});res.end(JSON.stringify(value));
 });
 const ollama=makeFake('ollama'),compatible=makeFake('compatible');
 await new Promise(r=>ollama.listen(0,'127.0.0.1',r));await new Promise(r=>compatible.listen(0,'127.0.0.1',r));
 const ollamaUrl=`http://127.0.0.1:${ollama.address().port}`,compatibleUrl=`http://127.0.0.1:${compatible.address().port}`;
 const data=path.join(fixture,'data');fs.mkdirSync(data);
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:ollamaUrl,ollama_url:ollamaUrl,llm_model:'writer:latest',legacy_sources:{}}));
 const executable=process.argv[2]?path.resolve(process.argv[2]):path.join(root,'sviluppo/target/release/nl-video-suite.exe');
 const port=await freePort(),base=`http://127.0.0.1:${port}`;
 let server,browser;
 const start=async()=>{server=spawn(executable,['--port',String(port),'--data-dir',data],{windowsHide:true,stdio:'ignore'});for(let n=0;n<150;n++){try{if((await fetch(base+'/api/providers')).ok)return;}catch{}await sleep(100);}throw Error('Rust server did not become ready');};
 const stop=async()=>{if(server&&!server.killed){const ended=new Promise(r=>server.once('exit',r));server.kill();await ended;}};
 const api=async(route,method='GET',body,status=200)=>{const r=await fetch(base+route,{method,headers:{'Content-Type':'application/json'},body:body===undefined?undefined:JSON.stringify(body)});assert.equal(r.status,status,`${method} ${route}: ${await r.clone().text()}`);return r.json();};
 try{
  await start();
  const original=await api('/api/settings'),initial=await api('/api/providers');
  assert.equal(initial.profiles.length,1);assert.equal(initial.active_id,'ollama-default');
  const probe=await api('/api/providers/models','POST',{provider:'openai_compatible',base_url:compatibleUrl});
  assert.equal(probe.models[0].name,'Local Author Q4.gguf');
  assert.deepEqual(await api('/api/settings'),original,'catalog probe must not change active configuration');
  assert(requests.filter(r=>r.kind==='compatible').every(r=>r.method==='GET'&&r.path==='/v1/models'));
  browser=await chromium.launch({channel:'msedge',headless:true});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  // Hold an old services response until after typing: it must never overwrite
  // a newly edited value, even when local/provider discovery responds late.
  let releaseServices,servicesCaptured,heldServices=false;
  const servicesGate=new Promise(resolve=>{releaseServices=resolve;});
  const servicesSeen=new Promise(resolve=>{servicesCaptured=resolve;});
  await page.route('**/api/services',async route=>{
   if(route.request().method()==='GET'&&!heldServices){heldServices=true;const response=await route.fetch();servicesCaptured();await servicesGate;await route.fulfill({response});}
   else await route.continue();
  });
  await page.goto(base);await navigate(page,'system');
  await servicesSeen;
  await page.waitForFunction(()=>document.querySelector('#service-comfy-reserve')?.value==='1');
  await page.locator('#services-form').evaluate(form=>{for(let p=form.parentElement;p;p=p.parentElement)if(p.tagName==='DETAILS')p.open=true;});
  await page.locator('#service-comfy-reserve').fill('1.5');
  releaseServices();await sleep(250);
  assert.equal(await page.locator('#service-comfy-reserve').inputValue(),'1.5','late services GET must preserve unsaved input');
  await page.locator('#services-form').getByRole('button',{name:'Salva percorsi',exact:true}).click();
  await page.waitForFunction(()=>document.querySelector('#services-status').textContent.includes('Percorsi salvati'));
  assert.equal((await api('/api/services')).config.comfy_reserved_vram_gb,1.5);
  await page.waitForFunction(()=>[...document.querySelector('#llm-model').options].some(o=>o.value==='graphics:latest'));
  await page.locator('#model-graphics').selectOption('graphics:latest');
  await page.locator('#settings-form button').first().click();
  await page.waitForFunction(()=>document.querySelector('#notice').textContent==='Impostazioni applicate. Non serve riavviare la suite.');
  await page.locator('#add-provider-profile').click();await page.locator('#provider-name').fill('LM Studio sintetico');
  await page.locator('#llm-provider').selectOption('openai_compatible');
  await page.locator('#openai-base-url').fill(compatibleUrl);await page.locator('#openai-base-url').blur();
  await page.waitForFunction(()=>[...document.querySelector('#llm-model').options].some(o=>o.value==='Local Author Q4.gguf'));
  assert.equal(await page.locator('#provider-launcher').inputValue(),'external');
  await page.locator('#llm-model').selectOption('Local Author Q4.gguf');await page.locator('#model-story').selectOption('Local Director Q4.gguf');
  await page.locator('#settings-form button').first().click();
  await page.waitForFunction(()=>document.querySelector('#provider-profile').selectedOptions[0]?.textContent.includes('LM Studio sintetico'));
  let registry=await api('/api/providers');assert.equal(registry.profiles.length,2);const compatibleId=registry.active_id;
  let config=await api('/api/settings');assert.equal(config.llm_provider,'openai_compatible');assert.equal(config.role_llm_models.story,'Local Director Q4.gguf');
  assert.equal(config.ollama_execution,'cpu');
  const project=await api('/api/projects','POST',{title:'Test provider sintetici'},201);
  const story=await api(`/api/projects/${project.id}/story/generate`,'POST',{premise:'Fixture non creativa, per test del trasporto'},202);
  let job;for(let n=0;n<100;n++){job=await api('/api/jobs/'+story.id);if(['succeeded','failed'].includes(job.status))break;await sleep(50);}assert.equal(job.status,'succeeded',job.message);
  const chat=requests.find(r=>r.path==='/v1/chat/completions');assert.equal(chat.body.model,'Local Director Q4.gguf');assert(!('options'in chat.body));assert(!('keep_alive'in chat.body));
  await page.locator('#provider-profile').selectOption('ollama-default');
  await page.waitForFunction(()=>document.querySelector('#model-graphics').value==='graphics:latest');
  assert.equal((await api('/api/settings')).llm_provider,'ollama');
  await page.locator('#provider-profile').selectOption(compatibleId);
  await page.waitForFunction(()=>document.querySelector('#model-story').value==='Local Director Q4.gguf');
  // Legacy /settings updates are retained in the active profile before switching.
  config=await api('/api/settings');await api('/api/settings','PUT',{comfy_url:config.comfy_url,ollama_url:config.ollama_url,llm_provider:config.llm_provider,openai_base_url:config.openai_base_url,llm_model:'Local Director Q4.gguf'});
  await api('/api/providers/activate','POST',{id:'ollama-default'});await api('/api/providers/activate','POST',{id:compatibleId});
  assert.equal((await api('/api/settings')).llm_model,'Local Director Q4.gguf');
  await api('/api/providers','PUT',{name:'Wrong launcher',provider:'openai_compatible',base_url:compatibleUrl,llm_model:'valid.gguf',launcher:'ollama'},400);
  const offlineUrl=`http://127.0.0.1:${await freePort()}`;
  registry=await api('/api/providers','PUT',{name:'Server esterno spento',provider:'openai_compatible',base_url:offlineUrl,llm_model:'manual Q4.gguf',launcher:'external'});
  const blocked=await api('/api/services/start','POST',{service:'llm'},400);assert.match(blocked.error,/gestito esternamente/);
  assert(!fs.existsSync(path.join(data,'services')),'no real provider process/log must be created');
  await page.reload();await navigate(page,'system');
  await page.waitForFunction(()=>document.querySelector('#service-comfy-reserve')?.value==='1.5');
  await page.waitForFunction(()=>document.querySelector('#models-status').textContent.includes('Provider non raggiungibile'));
  assert.equal(await page.locator('#llm-model').inputValue(),'manual Q4.gguf');
  await page.locator('#manual-llm-model').fill('modello manuale nuovo.gguf');await page.locator('#settings-form button').first().click();
  await page.waitForFunction(()=>document.querySelector('#manual-llm-model').value==='');
  assert.equal((await api('/api/settings')).llm_model,'modello manuale nuovo.gguf');
  await page.setViewportSize({width:390,height:844});const overflow=await page.evaluate(()=>({width:innerWidth,scroll:document.documentElement.scrollWidth,elements:[...document.querySelectorAll('body *')].filter(e=>e.getBoundingClientRect().right>innerWidth+1).map(e=>({tag:e.tagName,id:e.id,right:e.getBoundingClientRect().right,text:e.textContent.slice(0,90)})).slice(0,20)}));assert(overflow.scroll<=overflow.width+1,'provider profile mobile overflow: '+JSON.stringify(overflow));
  assert.deepEqual(errors,[]);
  await browser.close();browser=null;const beforeRestart=await api('/api/providers');await stop();await start();assert.deepEqual(await api('/api/providers'),beforeRestart);
  console.log('PASS providers: add/select/persist profiles, automatic selected-only catalogs, role routing, CPU safety, offline/manual model, no service launch, browser desktop/mobile');
 }finally{
  if(browser)await browser.close();await stop();await new Promise(r=>ollama.close(r));await new Promise(r=>compatible.close(r));
  if(path.basename(fixture).startsWith('nl-provider-profiles-test-')&&path.dirname(fixture)===os.tmpdir())fs.rmSync(fixture,{recursive:true,force:true,maxRetries:5,retryDelay:100});
 }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
