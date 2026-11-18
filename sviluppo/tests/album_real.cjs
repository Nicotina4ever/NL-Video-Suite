'use strict';
// Opt-in local image inference on an EXISTING isolated album. No import, ASR, downloads or cloud.
const {spawn}=require('node:child_process'),fs=require('node:fs'),path=require('node:path'),net=require('node:net'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../..'),sleep=ms=>new Promise(r=>setTimeout(r,ms));
(async()=>{
 const receipt=path.resolve(process.argv[2]||'');assert(receipt.startsWith(path.join(__dirname,'artifacts/album')+path.sep),'Only isolated album test data');const prior=JSON.parse(fs.readFileSync(receipt));assert(prior.passed,'Resume a complete technical test, not partially imported data');
 const data=prior.data,id=prior.project_id,out=path.dirname(receipt),report={started_at:new Date().toISOString(),passed:false,real_ai_inference:false,human_artistic_acceptance:false,model:'qwen3.5:9b',reused_project:id};
 // Overwrite ONLY isolated fixture configuration after the technical harness stopped.
 fs.writeFileSync(path.join(data,'settings.json'),JSON.stringify({ollama_url:'http://127.0.0.1:11434',comfy_url:'http://127.0.0.1:8188',legacy_sources:{},resources:{profile:'cpu_first',reserve_ram_mib:2048,llm_threads:4},role_llm_models:{vision:report.model,story:report.model}}));
 const sock=net.createServer();await new Promise(r=>sock.listen(0,'127.0.0.1',r));const port=sock.address().port;await new Promise(r=>sock.close(r));const base='http://127.0.0.1:'+port;
 const server=spawn(path.join(out,'suite-test.exe'),['--port',String(port),'--data-dir',data],{cwd:root,env:{...process.env,NL_SUITE_ROOT:root},windowsHide:true});let log='';server.stdout.on('data',v=>log+=v);server.stderr.on('data',v=>log+=v);
 const api=async(p,m='GET',b)=>{const r=await fetch(base+p,{method:m,headers:b?{'Content-Type':'application/json'}:{},body:b?JSON.stringify(b):undefined}),v=await r.json();assert(r.ok,JSON.stringify(v));return v;};
 const save=()=>fs.writeFileSync(path.join(out,'real-vision-report.json'),JSON.stringify(report,null,2));
 try{
  let ready=false;for(let i=0;i<100&&!ready;i++){await sleep(100);try{ready=(await fetch(base+'/api/projects')).ok;}catch{}}assert(ready);
  const resource=await api('/api/v1/resources'),catalog=await api('/api/v1/album/models'),entry=catalog.models.find(v=>v.name===report.model);assert(entry,'Previously installed image-capable model required');report.admission={memory:resource.memory,ollama_resident:resource.ollama_resident,model:entry};save();
  const ps=await fetch('http://127.0.0.1:11434/api/ps').then(r=>r.json());
  if(ps.models.length||resource.memory.available_bytes<entry.size+(1536+2048)*1048576){report.blocked=true;report.reason=ps.models.length?'Shared Ollama model active; no load/unload':'Conservative RAM admission; no image inference started';save();console.log('BLOCKED '+path.join(out,'real-vision-report.json'));return;}
  const p=await api('/api/projects/'+id),job=await api(`/api/projects/${id}/montage/album/analyze`,'POST',{expected_updated_at:p.updated_at,mode:'vision',model:report.model});report.job_id=job.id;save();let j=job;
  while(['queued','running'].includes(j.status)){await sleep(1000);j=await api('/api/jobs/'+job.id);report.current=j;save();console.log(j.message);}
  const result=await api('/api/projects/'+id);report.photos=Object.entries(result.montage.album.photos).map(([asset_id,v])=>({asset_id,...v}));report.real_ai_inference=report.photos.some(v=>v.model===report.model);report.passed=j.status==='succeeded'&&j.result.errors===0;report.completed_at=new Date().toISOString();save();assert(report.passed,'Review real per-photo errors before continuing');
 }finally{if(server.exitCode===null){server.kill();await new Promise(r=>server.once('exit',r));}fs.writeFileSync(path.join(out,'real-vision-server.log'),log);save();}
 console.log('REAL ALBUM EVIDENCE '+path.join(out,'real-vision-report.json'));
})().catch(e=>{console.error(e);process.exitCode=1;});
