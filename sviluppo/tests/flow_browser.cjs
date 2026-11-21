const {navigate}=require('./navigation.cjs');
// Actual Rust server + actual FFmpeg/cache-hit bridge + Edge. AI endpoints are
// explicitly fake. Verifies the entire reviewed workflow stays in one panel.
const {chromium}=require('playwright');
const {spawn,execFileSync}=require('node:child_process');
const fs=require('node:fs'),path=require('node:path'),os=require('node:os');
const assert=require('node:assert/strict'),http=require('node:http'),net=require('node:net');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
const {projectDir}=require('./project_folder.cjs');
async function port(){const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const p=socket.address().port;await new Promise(r=>socket.close(r));return p;}

async function main(){
 const temp=fs.mkdtempSync(path.join(os.tmpdir(),'nl-flow-browser-'));
 assert.equal(path.dirname(path.resolve(temp)),path.resolve(os.tmpdir()));assert(path.basename(temp).startsWith('nl-flow-browser-'));
 const exe=path.resolve(process.argv[2]||path.join(root,'sviluppo/target/release/nl-video-suite.exe'));
 const ffmpeg=path.join(root,'app/tools/ffmpeg.exe'),python=path.join(root,'runtime/python/python.exe');
 const source=path.join(temp,'synthetic.mp4'),data=path.join(temp,'data'),workflows=path.join(temp,'workflows');fs.mkdirSync(data);fs.mkdirSync(workflows);
 execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','color=navy:s=160x90:r=10:d=25','-f','lavfi','-i','sine=frequency=440:duration=25','-c:v','libx264','-threads','1','-pix_fmt','yuv420p','-c:a','aac','-shortest',source],{windowsHide:true});
 const png=execFileSync(ffmpeg,['-v','error','-f','lavfi','-i','color=green:s=96x64','-frames:v','1','-f','image2pipe','-c:v','png','pipe:1'],{windowsHide:true});
 const aiErrors=[];let graphicsCalls=0,chatCalls=0;
 const ai=http.createServer((req,res)=>{let body='';const json=value=>{res.setHeader('Content-Type','application/json');res.end(JSON.stringify(value));};req.on('data',c=>body+=c);req.on('end',()=>{try{
  if(req.url==='/api/tags')return json({models:[{name:'fake-flow-model'}]});
  if(req.url==='/api/ps')return json({models:[]});
  if(req.url==='/system_stats')return json({system:{os:'fixture'},devices:[]});
  if(req.url==='/queue')return json({queue_running:[],queue_pending:[]});
  if(req.url==='/object_info')return json({CLIPTextEncode:{input:{required:{text:['STRING']}},output:['CONDITIONING']},SaveImage:{input:{required:{filename_prefix:['STRING']}},output_node:true,output:[]}});
  if(req.url.startsWith('/history/'))return json({'fixture-prompt':{status:{completed:true,status_str:'success'},outputs:{'2':{images:[{filename:'fixture.png',type:'output',subfolder:''}]}}}});
  if(req.url.startsWith('/view')){res.setHeader('Content-Type','image/png');return res.end(png);}
  if(req.url==='/prompt')return json({prompt_id:'fixture-prompt'});
  assert.equal(req.url,'/api/chat');const b=JSON.parse(body);assert.equal(b.keep_alive,0);assert.equal(b.options.num_gpu,0);
  let value;if(b.messages[0].role==='system'&&b.messages[0].content.includes('Sei un editor video')){graphicsCalls++;const contexts=JSON.parse(b.messages[1].content);assert.equal(contexts.length,2);value={topic:'Fixture illustrativa',keywords:['voce'],graphics:[{source_clip_id:contexts[0].clip_id,offset_ms:100,duration_ms:700,prompt:'one illustrative microphone, close-up, soft light, no text',caption:'Illustrazione da revisionare',evidence:contexts[0].timed_speech[0].text}]};}
  else{chatCalls++;value={clips:[{title:'Proposta di collaudo',start_seg:1,end_seg:3,score:8,reason:'Risposta simulata: non è un giudizio creativo'}]};}
  return json({message:{content:JSON.stringify(value)}});
 }catch(error){aiErrors.push(error.message);res.statusCode=500;json({error:error.message});}});});
 await new Promise(r=>ai.listen(0,'127.0.0.1',r));const aiUrl=`http://127.0.0.1:${ai.address().port}`;
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:aiUrl,ollama_url:aiUrl,llm_model:'fake-flow-model',legacy_sources:{},workflow_roots:[workflows]}));
 fs.writeFileSync(path.join(workflows,'image-fixture.json'),JSON.stringify({'1':{class_type:'CLIPTextEncode',_meta:{title:'Positive (Input:prompt)'},inputs:{text:'old'}},'2':{class_type:'SaveImage',inputs:{filename_prefix:'fixture'}}}));
 const base=`http://127.0.0.1:${await port()}`;const server=spawn(exe,['--port',new URL(base).port,'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true,stdio:'ignore'});let browser;
 const request=async(url,method='GET',body)=>{const response=await fetch(base+url,{method,headers:body?{'Content-Type':'application/json'}:{},body:body?JSON.stringify(body):undefined});const result=await response.json();assert(response.ok,JSON.stringify(result));return result;};
 async function waitJob(job){for(let i=0;i<500;i++){const j=await request('/api/jobs/'+job.id);if(j.status==='succeeded')return j;assert.notEqual(j.status,'failed',j.message);await sleep(40);}throw Error('job timeout');}
 try{
  for(let i=0;i<200;i++){try{await request('/api/projects');break;}catch{await sleep(50);}}
  const project=await request('/api/projects','POST',{title:'Collaudo percorso guidato',aspect_ratio:'16:9'}),prefix='/api/projects/'+project.id;
  await waitJob(await request(prefix+'/assets','POST',{path:source}));const p=await request(prefix),asset=p.assets[0];
  const cache=path.join(projectDir(data,project.id),'analysis',asset.id);fs.mkdirSync(cache,{recursive:true});
  const words=Array.from({length:48},(_,i)=>({start:i*.5,end:i*.5+.45,word:i%8===7?'microfono.':'voce'}));
  const transcript={language:'it',duration:25,segments:[{start:0,end:24,text:'Una voce al microfono'}],words};
  const script="import os,sys,json,pathlib;os.environ['CLIPSAI_WHISPER_MODEL']=str(pathlib.Path('models/asr/faster-whisper-large-v3').resolve());sys.path.insert(0,str(pathlib.Path('engines/clipsai').resolve()));import motore;v,w,p=sys.argv[1:];x=json.loads(p);x['_fingerprint']=motore.video_fingerprint(v);x['_signature']=motore._cache_signature('it');pathlib.Path(w,pathlib.Path(v).stem+'_trascrizione.json').write_text(json.dumps(x),encoding='utf-8')";
  execFileSync(python,['-c',script,path.join(projectDir(data,project.id),'assets',asset.file_name),cache,JSON.stringify(transcript)],{cwd:root,windowsHide:true});
  browser=await chromium.launch({channel:'msedge',headless:true});const page=await browser.newPage({viewport:{width:1500,height:1000}}),errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.addInitScript(id=>localStorage.setItem('nl-project',id),project.id);await page.goto(base);await navigate(page,'pipeline');
  await page.waitForFunction(()=>document.querySelector('#flow-asset').options.length===1);
  await page.locator('#flow-start').click();await page.locator('[data-flow-cut]').waitFor({timeout:30000});
  assert.equal(await page.locator('#tab-pipeline').getAttribute('class'),'tab active');assert.equal(chatCalls,1);
  await page.locator('[data-flow-preview]').click();await page.waitForFunction(()=>document.querySelector('#flow-preview').readyState>=2);
  assert(await page.locator('#flow-preview').evaluate(v=>v.videoWidth>0));
  await page.locator('[data-flow-ranges]').fill('[[0,1],[2,3]]');await page.locator('[data-flow-cut]').uncheck();await page.locator('#flow-refresh').click();await page.waitForTimeout(400);
  assert.equal(await page.locator('[data-flow-ranges]').inputValue(),'[[0,1],[2,3]]');assert(!await page.locator('[data-flow-cut]').isChecked());
  await page.locator('[data-flow-cut]').check();await page.locator('#flow-auto-graphics').check();await page.locator('#flow-workflow').selectOption({label:'image-fixture'});
  await page.locator('#flow-apply').click();await page.locator('[data-flow-approve]').waitFor({timeout:30000});assert.equal(graphicsCalls,1);
  assert.equal((await request(prefix)).timeline.clips.length,2,'unapproved image entered timeline');
  assert(await page.locator('#flow-export').isHidden());await page.locator('[data-flow-approve]').click();
  await page.locator('#flow-export').waitFor();await page.locator('#flow-export').click();await page.locator('#flow-download a').waitFor({timeout:30000});
  assert.equal((await request(prefix)).timeline.clips.length,3);assert.equal(await page.locator('#tab-pipeline').getAttribute('class'),'tab active');
  fs.mkdirSync(path.join(__dirname,'artifacts'),{recursive:true});await page.screenshot({path:path.join(__dirname,'artifacts','percorso-guidato-desktop.png'),fullPage:true});
  await page.locator('[data-flow-export-preview]').click();await page.waitForFunction(()=>document.querySelector('#flow-preview').readyState>=2);
  await page.reload();await navigate(page,'pipeline');await page.locator('#flow-download a').waitFor();assert.equal(chatCalls,1);assert.equal(graphicsCalls,1);
  await page.setViewportSize({width:390,height:844});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1),'guided-flow mobile overflow');
  await page.screenshot({path:path.join(__dirname,'artifacts','percorso-guidato-mobile.png'),fullPage:true});
  assert.deepEqual(errors,[]);assert.deepEqual(aiErrors,[]);
  console.log('PASS Edge: one panel analyze→preview→edited review→automatic graphics→image approval→actual export; drafts preserved; reload state; mobile; zero JS errors');
 }finally{
  if(browser)await browser.close();if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}await new Promise(r=>ai.close(r));
  assert.equal(path.dirname(path.resolve(temp)),path.resolve(os.tmpdir()));assert(path.basename(temp).startsWith('nl-flow-browser-'));fs.rmSync(temp,{recursive:true,force:true,maxRetries:10,retryDelay:100});
 }
}
main().catch(error=>{console.error(error);process.exitCode=1;});
