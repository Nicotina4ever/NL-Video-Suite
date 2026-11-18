'use strict';
// Read-only UI QA on the retained authentic demo. No inference, export or copy
// of its media. Edits/biography stay untouched; only filter/details UI changes.
const {chromium}=require('playwright'),{spawn}=require('node:child_process'),fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,ok=Boolean){let v;for(let n=0;n<300;n++){v=await fn();if(ok(v))return v;await sleep(100);}throw Error('timeout '+JSON.stringify(v));}
(async()=>{
 const demo=path.join(__dirname,'artifacts/album/2026-11-05T01-42-05-381Z/replay-reviewed'),receipt=JSON.parse(fs.readFileSync(path.join(demo,'report.json'))),data=receipt.data,dir=path.join(data,'projects',receipt.project_id),projectFile=path.join(dir,'project.json'),before=fs.readFileSync(projectFile);
 const out=path.join(__dirname,'artifacts/album-existing-ux',new Date().toISOString().replace(/[:.]/g,'-'));fs.mkdirSync(out,{recursive:true});
 const exe=path.resolve(process.argv[2]||path.join(root,'sviluppo/target/release/nl-video-suite-planner.exe'));
 const report={passed:false,real_ai_inference:false,render:false,real_photographs:true,human_artistic_acceptance:false,sha256:crypto.createHash('sha256').update(fs.readFileSync(exe)).digest('hex')};
 const socket=net.createServer();await new Promise(r=>socket.listen(0,'127.0.0.1',r));const port=socket.address().port;await new Promise(r=>socket.close(r));const base='http://127.0.0.1:'+port;
 let native,browser,page;
 try{
  native=spawn(exe,['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true,stdio:'ignore'});
  await until(async()=>{try{return(await fetch(base+'/api/projects')).ok;}catch{return false;}});
  browser=await chromium.launch({channel:'msedge',headless:true});page=await browser.newPage({viewport:{width:1366,height:900}});const errors=[];page.on('pageerror',e=>errors.push(e.message));
  // Fail if a regression makes merely reviewing the retained album submit IA.
  const writes=[];page.on('request',r=>{if(r.method()!=='GET'&&/album\/analyze|planner\/propose|\/export$/.test(r.url()))writes.push(r.url());});
  await page.goto(base);await page.locator('#projects').selectOption(receipt.project_id);await page.waitForFunction(id=>state.project?.id===id,receipt.project_id);await page.locator('#montage-studio').evaluate(e=>e.open=true);await page.locator('#album-studio').evaluate(e=>e.open=true);
  assert.equal(await page.locator('#album-protagonist').inputValue(),'');assert((await page.locator('#album-facts').inputValue()).includes('committente'));
  await page.locator('#album-view').selectOption('all');const count=await page.locator('[data-album-asset]').count();assert(count>=8&&count<=24);
  await page.locator('#album-photos').evaluate(e=>window.scrollBy(0,e.getBoundingClientRect().top-80));await page.screenshot({path:path.join(out,'authentic-gallery.png')});
  const card=page.locator('[data-album-asset]').first();await card.locator('.album-extra').evaluate(e=>e.open=true);assert.equal(await card.locator('[data-album-field="protagonist_confirmed"]').isChecked(),false);await card.scrollIntoViewIfNeeded();await page.screenshot({path:path.join(out,'authentic-inspection.png')});
  await page.locator('#album-order').selectOption('date');assert((await page.locator('#album-photos').textContent()).includes('Data di scatto sconosciuta'));await page.setViewportSize({width:1024,height:640});assert(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+2));await page.screenshot({path:path.join(out,'authentic-1024.png')});
  assert.deepEqual(writes,[]);assert.deepEqual(errors,[]);report.checks=['Authentic photographs in compact gallery; details reachable; unknown identity and client role retained','Confirmed-date gallery sorting never invents capture dates','No album/planner inference or export requested, no project editing; 1024px no horizontal overflow'];report.passed=true;
 }catch(e){report.error=e.stack;throw e;}finally{await browser?.close();if(native?.exitCode===null){native.kill();await new Promise(r=>native.once('exit',r));}report.project_bytes_unchanged=fs.readFileSync(projectFile).equals(before);if(!report.project_bytes_unchanged)report.passed=false;fs.writeFileSync(path.join(out,'report.json'),JSON.stringify(report,null,2));}
 assert(report.passed);console.log('PASS album_existing_ux '+out);
})().catch(e=>{console.error(e);process.exitCode=1;});
