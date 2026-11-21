'use strict';
// Renaming a project must follow its readable folder when one exists; legacy
// UUID folders keep their stable name until the startup migration owns them.
const {navigate}=require('./navigation.cjs');
const {chromium}=require('playwright'),{spawn}=require('node:child_process'),fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));async function until(fn,ok=Boolean){let v;for(let i=0;i<600;i++){v=await fn();if(ok(v))return v;await sleep(100);}throw Error('timeout '+JSON.stringify(v));}
const {projectDir}=require('./project_folder.cjs');
(async()=>{
 const out=path.join(__dirname,'artifacts/project-rename',new Date().toISOString().replace(/[:.]/g,'-')),data=path.join(out,'data');fs.mkdirSync(data,{recursive:true});
 const now=new Date().toISOString(),legacyId=crypto.randomUUID(),legacyDir=path.join(data,'projects',legacyId);fs.mkdirSync(legacyDir,{recursive:true});
 fs.writeFileSync(path.join(legacyDir,'project.json'),JSON.stringify({id:legacyId,title:'Legacy da rinominare',created_at:now,updated_at:now,aspect_ratio:'16:9',assets:[]}));
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({comfy_url:'http://127.0.0.1:9',ollama_url:'http://127.0.0.1:9',legacy_sources:{}}));
 const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));const base='http://127.0.0.1:'+port;let server,browser,page,logs='';const report={passed:false,inference:false,checks:[]};
 const request=async(url,method='GET',body,status=200)=>{const r=await fetch(base+url,{method,headers:body?{'Content-Type':'application/json'}:{},body:body?JSON.stringify(body):undefined});const v=await r.json().catch(()=>({}));assert.equal(r.status,status,url+' '+JSON.stringify(v));return v;};
 try{
  server=spawn(path.resolve(process.argv[2]),['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});server.stdout.on('data',b=>logs+=b);server.stderr.on('data',b=>logs+=b);
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  const created=await request('/api/projects','POST',{title:'Titolo iniziale QA',aspect_ratio:'16:9'},201);
  const firstDir=projectDir(data,created.id);assert(path.basename(firstDir).startsWith('Titolo iniziale QA--'));
  const short=created.id.replace(/-/g,'').slice(0,8);
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1280,height:850}});const errors=[];page.on('pageerror',e=>errors.push(e.message));await page.goto(base);await page.locator('#projects').selectOption(created.id);await page.waitForFunction(id=>state.project?.id===id,created.id);await navigate(page,'projects');
  await page.locator('#project-rename-open').click();await page.locator('#project-rename-title').fill('Titolo rinominato QA');await page.locator('#project-rename-save').click();
  const renamed=await until(()=>{const p=projectDir(data,created.id);return path.basename(p)==='Titolo rinominato QA--'+short?p:null;});
  assert(!fs.existsSync(firstDir),'the previous folder name must be gone');
  assert.equal((await fs.promises.readFile(path.join(renamed,'.project-id'),'utf8')).trim(),created.id);
  assert((await fs.promises.readFile(path.join(renamed,'project.json'),'utf8')).includes('Titolo rinominato QA'));
  const receipt=JSON.parse(fs.readFileSync(path.join(data,'project-folders-migration.json'),'utf8'));
  assert(receipt.items.some(i=>i.from==='Titolo iniziale QA--'+short&&i.to==='Titolo rinominato QA--'+short));
  await until(()=>page.evaluate(id=>document.querySelector('#projects option[value="'+id+'"]')?.textContent,created.id),t=>t==='Titolo rinominato QA');
  const stale=await request('/api/projects/'+created.id+'/title','PUT',{title:'Attacco con revisione vecchia',expected_updated_at:created.updated_at},409);assert(stale);
  await page.screenshot({path:path.join(out,'renamed.png')});report.checks.push('GUI rename follows the readable folder, keeps .project-id, writes the receipt, refreshes the picker and refuses stale edits; no media copied');
  await page.locator('#projects').selectOption(legacyId);await page.waitForFunction(id=>state.project?.id===id,legacyId);
  await page.locator('#project-rename-open').click();await page.locator('#project-rename-title').fill('Legacy rinominato GUI');await page.locator('#project-rename-save').click();
  const legacyShort=legacyId.replace(/-/g,'').slice(0,8);
  await until(()=>{try{return JSON.parse(fs.readFileSync(path.join(data,'projects','Legacy rinominato GUI--'+legacyShort,'project.json'),'utf8')).title;}catch{return null;}},t=>t==='Legacy rinominato GUI');
  assert(!fs.existsSync(legacyDir),'the startup migration already left the UUID folder behind');
  const receipt2=JSON.parse(fs.readFileSync(path.join(data,'project-folders-migration.json'),'utf8'));
  assert(receipt2.items.some(i=>i.from===legacyId&&i.to==='Legacy da rinominare--'+legacyShort),'startup migration receipt uuid→title--short');
  assert(receipt2.items.some(i=>i.from==='Legacy da rinominare--'+legacyShort&&i.to==='Legacy rinominato GUI--'+legacyShort),'GUI rename receipt follows the new title');
  await page.reload();await until(()=>request('/api/projects'),p=>p.length===2);const titles=(await request('/api/projects')).map(p=>p.title).sort();assert.deepEqual(titles,['Legacy rinominato GUI','Titolo rinominato QA']);assert.deepEqual(errors,[]);
  report.checks.push('Startup migration makes the UUID folder readable; GUI rename then follows the title with a receipt chain for both steps; both projects reopen with new titles and zero JS errors');
  report.passed=true;
 }catch(e){report.error=e.stack;await page?.screenshot({path:path.join(out,'failure.png')}).catch(()=>{});throw e;}finally{await browser?.close();if(server?.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));fs.writeFileSync(path.join(out,'server.log'),logs);if(report.passed){fs.rmSync(data,{recursive:true,force:true});}console.log(JSON.stringify({passed:report.passed,report:path.join(out,'report.json')}));}
})().catch(e=>{console.error(e.stack);process.exitCode=1;});
