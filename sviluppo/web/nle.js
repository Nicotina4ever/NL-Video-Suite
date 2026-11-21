'use strict';
// Shared by source monitor, NLE and guided work: one proxy preparation per asset.
window.MediaPreview=(()=>{
  const requests=new Map(),delay=ms=>new Promise(resolve=>setTimeout(resolve,ms));
  async function request(url,method='GET'){const r=await fetch(url,{method});const body=await r.json();if(!r.ok)throw Error(body.error||'Anteprima non disponibile');return body;}
  function resolve(projectId,asset,{force=false,onStatus=()=>{}}={}){
    const key=projectId+'/'+asset.id;let entry=requests.get(key);
    if(force){requests.delete(key);entry=null;}
    if(entry){if(!entry.done)entry.listeners.add(onStatus);onStatus(entry.message||'');return entry.promise;}
    entry={listeners:new Set([onStatus]),message:''};requests.set(key,entry);
    const status=message=>{entry.message=message;entry.listeners.forEach(fn=>fn(message));};
    entry.promise=(async()=>{
      const endpoint=`/api/projects/${projectId}/assets/${asset.id}/playback`;
      let info=force?await request(endpoint+'?refresh=true','POST'):await request(endpoint);
      if(info.mode==='proxy_required'){status(info.message);info=await request(endpoint,'POST');}
      while(info.mode==='preparing'){status(info.message||'Anteprima compatibile in preparazione…');await delay(1000);info=await request(endpoint);}
      if(info.mode==='failed')throw Error(info.message+' · Usa «Rigenera anteprima» per riprovare.');
      if(!info.url)throw Error('Indirizzo anteprima mancante');status(info.mode==='proxy'?'Anteprima leggera · export dall’originale':'');return info.url;
    })().then(url=>{entry.done=true;entry.listeners.clear();return url;}).catch(error=>{status(error.message);requests.delete(key);throw error;});return entry.promise;
  }
  return{resolve};
})();
// The canonical timeline is the same project snapshot used by Rust/FFmpeg.
// No source file is edited; every pointer gesture commits one undoable change.
window.NLE=(()=>{
  const $=s=>document.querySelector(s), clone=v=>JSON.parse(JSON.stringify(v));
  const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const defaults=()=>({x:.5,y:.5,scale:1,rotation:0,fit:'contain',focus_x:.5,focus_y:.5});
  const s={project:null,selected:null,objectSelected:null,objectDirty:false,time:0,pps:60,playing:false,awaitingPlay:false,snap:true,tool:'select',players:new Map(),images:new Map(),draft:null,gesture:null,audio:null,origin:0,raf:0,fingerprint:'',api:null,showWave:true};
 try{s.showWave=localStorage.getItem('nl-nle-wave')!=='0';}catch{}
  const project=()=>s.project, timeline=()=>s.draft||project()?.timeline;
  const asset=id=>project()?.assets.find(a=>a.id===id);
  const clip=id=>timeline()?.clips.find(c=>c.id===id);
  const track=id=>timeline()?.tracks.find(t=>t.id===id);
  const end=()=>Math.max(0,...(timeline()?.clips||[]).map(c=>c.start_ms+c.duration_ms),...(project()?.graphic_objects||[]).map(o=>o.start_ms+o.duration_ms),...(project()?.motion?.graphics||[]).map(g=>{const c=timeline()?.clips.find(c=>c.id===g.anchor_clip_id);return c?Math.max(0,Math.min(c.start_ms+c.duration_ms,c.start_ms+g.anchor_source_ms-c.trim_in_ms+g.duration_ms)):g.start_ms+g.duration_ms;}));
  const fps=()=>{const p=project()?.social?.profiles.find(p=>p.id===project()?.social?.selected);return p?p.fps.numerator/p.fps.denominator:30;};
  const frame=ms=>Math.round(ms/1000*fps())/fps()*1000;
  const clamp=(n,a,b)=>Math.max(a,Math.min(b,n));
  const sourceUrl=id=>`/api/projects/${project().id}/assets/${id}/file`;
  const cacheUrl=(id,kind)=>`/api/projects/${project().id}/assets/${id}/${kind}`;
  const selected=()=>clip(s.selected);
  const object=id=>(project()?.graphic_objects||[]).find(o=>o.id===id);
  const trackForObject=o=>timeline()?.tracks.find(t=>t.kind===(o.media_kind==='audio'?'audio':'overlay'));
  function objectBlocks(t){return(project().graphic_objects||[]).filter(o=>o.status!=='approved'&&trackForObject(o)?.id===t.id).map(o=>`<div role="button" tabindex="0" class="nle-object ${o.id===s.objectSelected?'selected':''}" data-nle-object="${o.id}" style="left:${118+o.start_ms/1000*s.pps}px;width:${Math.max(3,o.duration_ms/1000*s.pps)}px" title="${esc(o.title)} · trascina per spostare, bordi per durata"><i class="nle-handle" data-edge="start"></i><b>IA · ${esc(o.title)}</b><small>${esc(objectStatus(o))}</small><i class="nle-handle end" data-edge="end"></i></div>`).join('');}
  function selectObject(id){if(s.objectDirty&&s.objectSelected!==id&&!confirm('Scartare le modifiche non salvate all’oggetto?'))return;s.objectSelected=id;s.objectDirty=false;s.selected=null;render();inspect();draw();}
  function objectStatus(o){const labels={draft:'Bozza',describing:'Descrizione in corso',described:'Prompt pronto',generating:'Generazione in corso',review:'Da approvare',approved:'Approvato',failed:'Errore'};const job=s.api?.getJobs?.().find(j=>j.id===o.job_id);return (job?.status==='failed'?'Lavoro interrotto o non riuscito':labels[o.status]||o.status)+(o.last_error?'\n'+o.last_error:'');}
  function inspectObject(){
    const o=object(s.objectSelected);$('#object-inspector').hidden=!o;$('#clip-inspector').hidden=Boolean(o);if(!o)return false;
    $('#inspector-name').textContent='Oggetto IA · '+o.title;
    const job=s.api?.getJobs?.().find(j=>j.id===o.job_id),busy=job&&['queued','running'].includes(job.status);
    $('#object-inspector').disabled=Boolean(busy);
    if(!s.objectDirty){
      $('#object-title').value=o.title;$('#object-start').value=(o.start_ms/1000).toFixed(3);$('#object-duration').value=(o.duration_ms/1000).toFixed(3);
      $('#object-description').value=o.description;$('#object-prompt').value=o.prompt;
      const workflows=s.api.getWorkflows?.()||[];
      $('#object-workflow').innerHTML='<option value="">Scegli il workflow installato</option>'+workflows.map(w=>`<option value="${w.id}">${esc(w.name)}</option>`).join('');
      $('#object-workflow').value=o.workflow_id||'';window.NLObjectMedia?.render(o);
    }
    $('#object-status').textContent=objectStatus(o)+(busy?'\n'+job.message:'');
    $('#object-source').textContent=o.source_text||'Il prompt usa il parlato di questo intervallo e il contesto circostante. Puoi anche scriverlo manualmente.';
    const image=$('#object-review-image'),a=asset(o.asset_id);image.hidden=!a||a.kind!=='image';if(a?.kind==='image')image.src=sourceUrl(a.id);
    window.NLObjectMedia?.preview(a);$('#object-approve').disabled=o.status!=='review';return true;
  }
  async function newObject(kind='image'){
    if(!project())throw Error('Crea un progetto.');
    if(s.objectDirty&&!confirm('Scartare le modifiche non salvate all’oggetto?'))return;
    pause();const mediaKind=['image','video','audio'].includes(kind)?kind:'image',current=selected();
    const source=mediaKind==='video'?(current&&asset(current.asset_id)?.kind==='video'?current:timeline().clips.findLast(c=>asset(c.asset_id)?.kind==='video'&&track(c.track_id)?.visible&&c.start_ms<=s.time&&c.start_ms+c.duration_ms>s.time)):(current&&['video','audio'].includes(asset(current.asset_id)?.kind)?current:null);
    if(mediaKind==='video'&&!source)throw Error('Per partire da un frame, importa un video e posiziona il cursore su quel clip. Per un video da zero usa Grafica IA o Storia e scene.');
    const start=Math.round(source?clamp(s.time,source.start_ms,source.start_ms+source.duration_ms-1):s.time),wanted=mediaKind==='audio'?30000:3000;
    const duration=Math.max(1,Math.min(wanted,source?source.start_ms+source.duration_ms-start:wanted));
    const pid=project().id,p=await s.api.api(`/api/projects/${pid}/graphic-objects`,'POST',{title:mediaKind==='audio'?'Musica / audio IA':mediaKind==='video'?'Video IA da fotogramma':'Grafica IA',media_kind:mediaKind,start_ms:start,duration_ms:duration,source_clip_id:source?.id||null});
    if(project()?.id===pid){s.objectDirty=false;s.api.acceptProject(p);selectObject(p.graphic_objects.at(-1).id);s.api.notify('Bozza pronta. Scegli il workflow e compila il prompt oppure ricavalo dal parlato.');return p.graphic_objects.at(-1).id;}
  }
  async function saveObject(){const o=object(s.objectSelected);if(!o)return;const pid=project().id;const body={title:$('#object-title').value,start_ms:Math.round(Number($('#object-start').value)*1000),duration_ms:Math.round(Number($('#object-duration').value)*1000),description:$('#object-description').value,prompt:$('#object-prompt').value,...window.NLObjectMedia?.read(o)};const workflow=$('#object-workflow').value;if(workflow)body.workflow_id=workflow;const p=await s.api.api(`/api/projects/${pid}/graphic-objects/${o.id}`,'PUT',body);if(project()?.id===pid){s.objectDirty=false;s.api.acceptProject(p);}return o.id;}
  async function objectAction(action){const id=await saveObject();if(!id)return;const pid=project().id;await s.api.api(`/api/projects/${pid}/graphic-objects/${id}/${action}`,'POST',action==='generate'?{workflow_id:$('#object-workflow').value||null}:{});if(project()?.id===pid){await s.api.reloadProject();await s.api.refreshJobs();inspect();}s.api.notify({describe:'Descrizione e prompt dal parlato in coda: revisiona prima di generare.',generate:'Generazione in coda. L’oggetto richiederà la tua approvazione.',approve:'Output approvato e inserito in timeline.'}[action]);}
  async function removeObject(){const o=object(s.objectSelected);if(!o)return;const p=await s.api.api(`/api/projects/${project().id}/graphic-objects/${o.id}`,'DELETE');s.objectSelected=null;s.objectDirty=false;s.api.acceptProject(p);}
  function timecode(ms){const rate=fps(),nominal=Math.round(rate),f=Math.max(0,Math.floor(ms/1000*rate));return[Math.floor(f/(nominal*3600)),Math.floor(f/(nominal*60))%60,Math.floor(f/nominal)%60,f%nominal].map(n=>String(n).padStart(2,'0')).join(':');}
  function report(error){s.api?.notify(error.message||String(error),true);}
  function validTrack(c,t){const a=asset(c.asset_id);return t&&!t.locked&&(a?.kind==='audio'?t.kind==='audio':t.kind!=='audio');}
  async function unlinkAudio(){
    const c=selected();if(!c)throw Error('Seleziona un clip video.');
    const a=asset(c.asset_id);if(!a||a.kind!=='video'||!a.media?.has_audio)throw Error('Il clip selezionato non ha audio da scollegare.');
    const at=timeline().tracks.find(t=>t.kind==='audio');if(!at)throw Error('Nessuna traccia audio nel progetto.');
    if(timeline().clips.some(x=>x.track_id===at.id&&x.asset_id===c.asset_id&&x.start_ms===c.start_ms&&x.trim_in_ms===c.trim_in_ms&&x.duration_ms===c.duration_ms))throw Error('L’audio di questo clip è già scollegato.');
    await mutate(tl=>{
      const v=tl.clips.find(x=>x.id===c.id);if(!v)return;
      const copy={...JSON.parse(JSON.stringify(v)),id:crypto.randomUUID(),track_id:at.id,label:'Audio · '+(v.label||a.name),volume:Number(v.volume)||1};
      v.volume=0;tl.clips.push(copy);
    });
    s.api?.notify?.('Audio scollegato: clip indipendente sulla traccia audio, video muto.');
  }
  async function relinkAudio(){
    const c=selected();if(!c)throw Error('Seleziona il clip video da ricollegare.');
    const at=timeline().tracks.find(t=>t.kind==='audio');
    const match=timeline().clips.find(x=>x.track_id===at?.id&&x.asset_id===c.asset_id&&x.start_ms===c.start_ms&&x.trim_in_ms===c.trim_in_ms&&x.duration_ms===c.duration_ms&&String(x.label||'').startsWith('Audio · '));
    if(!match)throw Error('Nessun audio scollegato corrispondente a questo clip.');
    await mutate(tl=>{tl.clips=tl.clips.filter(x=>x.id!==match.id);const v=tl.clips.find(x=>x.id===c.id);if(v)v.volume=Math.max(1,Number(match.volume)||1);});
    s.api?.notify?.('Audio ricollegato al video.');
  }
  function configure(api){s.api=api;$('#nle-fit').insertAdjacentHTML('afterend','<label>Nuova traccia<select id="nle-track-kind"><option value="video">Video</option><option value="overlay">Grafiche</option><option value="audio">Audio</option></select></label><button id="nle-add-track" class="secondary">+ Traccia</button>');$('#nle-duplicate').insertAdjacentHTML('afterend','<button id="nle-unlink" class="secondary" title="Scollega l\'audio dal video (Ctrl+L)">Scollega audio</button><button id="nle-relink" class="secondary" title="Ricollega l\'audio scollegato">Ricollega</button><label class="check" title="Mostra la forma d\'onda sulle clip video"><input id="nle-wave" type="checkbox" checked>Waveform</label>');$('#nle-wave').checked=s.showWave;$('#nle-unlink').addEventListener('click',()=>unlinkAudio().catch(report));$('#nle-relink').addEventListener('click',()=>relinkAudio().catch(report));$('#nle-wave').addEventListener('change',e=>{s.showWave=e.target.checked;try{localStorage.setItem('nl-nle-wave',s.showWave?'1':'0');}catch{}render();});document.addEventListener('keydown',e=>{if(!e.ctrlKey||e.shiftKey||e.altKey||String(e.key).toLowerCase()!=='l')return;if(!document.querySelector('#tab-studio')?.classList.contains('active'))return;const tag=document.activeElement?.tagName;if(['INPUT','TEXTAREA','SELECT'].includes(tag))return;e.preventDefault();unlinkAudio().catch(report);});$('#inspector-name').insertAdjacentHTML('afterend','<button id="open-clip-object" class="secondary" hidden>Descrizione e prompt dell’oggetto</button>');bind();window.NLObjectMedia.configure({api,project,object:()=>object(s.objectSelected),time:()=>s.time,selected,save:saveObject,dirty:()=>{s.objectDirty=true;}});}
  function update(p){
    if(p?.id!==s.project?.id){pause();s.players.forEach(v=>{v.element.removeAttribute('src');v.element.load();v.node?.disconnect();v.gain?.disconnect();});s.players.clear();s.images.clear();s.selected=null;s.objectSelected=null;s.objectDirty=false;s.time=0;s.fingerprint='';}
    s.project=p;
    if(!p){$('#nle-content').innerHTML='';draw();inspect();return;}
    const next=JSON.stringify([p.timeline,p.graphic_objects,p.assets.map(a=>a.id),p.subtitles?.revision,p.motion?.revision]);
    if(next!==s.fingerprint){s.fingerprint=next;render();}
    if(!clip(s.selected))s.selected=null;
    for(const[id,player]of s.players)if(!p.timeline.clips.some(c=>c.id===id)){player.element.pause();player.element.removeAttribute('src');player.element.load();player.node?.disconnect();player.gain?.disconnect();s.players.delete(id);}
    inspect();draw();
  }
  function select(id){if(s.objectDirty&&!confirm('Scartare le modifiche non salvate all’oggetto?'))return;s.objectSelected=null;s.objectDirty=false;s.selected=id;document.querySelectorAll('.nle-clip').forEach(el=>el.classList.toggle('selected',el.dataset.nleClip===id));inspect();draw();}
  function render(){
    const tl=timeline();if(!tl)return;
    const width=Math.max(640,$('#nle-scroll').clientWidth-118,(end()/1000+3)*s.pps);
    const step=[.1,.2,.5,1,2,5,10,15,30,60,120,300,600].find(v=>v*s.pps>=65)||600;
    let ruler='';for(let t=0;t<=width/s.pps;t+=step)ruler+=`<span style="left:${t*s.pps}px">${timecode(t*1000).slice(3)}</span>`;
    const lanes=tl.tracks.map((t,i)=>{
      const clips=tl.clips.filter(c=>c.track_id===t.id).map(c=>{
        const a=asset(c.asset_id);if(!a)return'';
        const audioLane=t.kind==='audio'||a.kind==='audio';
        const media=audioLane?cacheUrl(a.id,'waveform'):a.kind==='image'?sourceUrl(a.id):cacheUrl(a.id,'thumbnail');
        const duration=a.media?.duration_ms||c.duration_ms;
        const style=audioLane?`background-size:${duration/1000*s.pps}px 100%;background-position:${-c.trim_in_ms/1000*s.pps}px center;`:'';
        const wave=a.kind!=='image'&&a.media?.has_audio&&s.showWave?`<div class="clip-wave" style="background-image:url('${cacheUrl(a.id,'waveform')}');background-size:${duration/1000*s.pps}px 100%;background-position:${-c.trim_in_ms/1000*s.pps}px center" aria-hidden="true"></div>`:'';
        return `<div class="nle-clip ${c.id===s.selected?'selected':''} ${t.locked?'locked':''}" tabindex="0" role="button" aria-label="Clip ${esc(c.label||a.name)}" data-nle-clip="${c.id}" data-kind="${a.kind}" style="left:${118+c.start_ms/1000*s.pps}px;width:${Math.max(3,c.duration_ms/1000*s.pps)}px" title="${esc(c.label||a.name)} · ${timecode(c.start_ms)}"><i class="nle-handle" data-edge="start"></i><b>${esc(c.label||a.name)}</b><small>${(c.duration_ms/1000).toFixed(2)} s${a.media?.has_audio&&c.volume?' · ♫':''}</small>${wave}<div class="clip-media" style="background-image:url('${media}');${style}"></div><i class="nle-handle end" data-edge="end"></i></div>`;
      }).join('');
      const name=t.name==='video'?'V1 · Video':t.name==='overlay'?'V2 · Grafiche':t.name==='audio'?'A1 · Audio':t.name;
      return `<div class="nle-lane ${!t.visible?'muted-lane':''}" data-lane="${t.id}" style="width:${width+118}px"><div class="lane-label"><span>${esc(name)}</span><div><button data-lane-visible="${t.id}" aria-pressed="${!t.visible}" title="Attiva/disattiva traccia">${t.kind==='audio'?'M':'Occhio'}</button><button data-lane-lock="${t.id}" aria-pressed="${t.locked}" title="Blocca traccia">${t.locked?'Bloccata':'Blocca'}</button></div></div>${clips}${objectBlocks(t)}</div>`;
    }).join('');
    $('#nle-content').style.width=`${width+118}px`;
    $('#nle-content').innerHTML=`<div class="nle-ruler" style="width:${width}px">${ruler}</div>${lanes}${window.MotionStudio?.lane(project(),s.pps,width)||''}${window.SubtitleStudio?.lane(project(),s.pps,width)||''}<div class="nle-playhead" id="nle-playhead"></div>${!tl.clips.length?'<div class="nle-empty">Trascina un video, una grafica o un audio dalla libreria su una traccia. Le immagini generate da ComfyUI entrano nella stessa libreria.</div>':''}`;
    $('#nle-duration').textContent=`${tl.clips.length} clip · ${timecode(end())}`;
    $('#nle-scroll').classList.toggle('nle-blade',s.tool==='blade');
    for(const [id,tool]of [['#nle-select-tool','select'],['#nle-blade-tool','blade']]){$(id).classList.toggle('selected-tool',s.tool===tool);$(id).setAttribute('aria-pressed',String(s.tool===tool));}syncHead();
  }
  function followHead(force=false){
    if(!force&&!s.playing)return;
    const scroll=$('#nle-scroll');if(!scroll||scroll.clientWidth<=150||!scroll.getClientRects().length)return;
    const x=118+s.time/1000*s.pps,left=scroll.scrollLeft+130,right=scroll.scrollLeft+scroll.clientWidth-24;
    if(x<left||x>right){const usable=Math.max(1,scroll.clientWidth-142);scroll.scrollLeft=clamp(x-130-usable*.2,0,Math.max(0,scroll.scrollWidth-scroll.clientWidth));}
  }
  function syncHead(){followHead();const head=$('#nle-playhead');if(head)head.style.left=`${118+s.time/1000*s.pps}px`;$('#program-time').textContent=timecode(s.time);if(document.activeElement!==$('#program-seek'))$('#program-seek').value=(s.time/1000).toFixed(2);}
  function inspect(){
    $('#open-clip-object').hidden=true;if(inspectObject())return;
    const c=selected(),a=c&&asset(c.asset_id),locked=!c||track(c.track_id)?.locked;
    document.dispatchEvent(new CustomEvent('nl:clip-selection',{detail:{clip_id:c?.id||null}}));
    $('#clip-inspector').disabled=Boolean(locked);$('#inspector-name').textContent=c?(c.label||a?.name):'Seleziona un clip nella timeline';
    if($('#clip-motion'))$('#clip-motion').value=c?.transform?.motion||'none';
    if($('#clip-fit')&&document.activeElement!==$('#clip-fit'))$('#clip-fit').value=c?(c.transform?.fit||'contain'):'';
    if(!c){$('#composition-advice').textContent='Seleziona un clip per modificarne adattamento e camera.';return;}const associated=(project().graphic_objects||[]).find(o=>o.asset_id===c.asset_id);$('#open-clip-object').hidden=!associated;$('#open-clip-object').dataset.object=associated?.id||'';
    const camera=c.transform?.camera_path,tuning=s.cameraTuning?.[c.id];if(document.activeElement!==$('#camera-behavior'))$('#camera-behavior').value=tuning?.behavior||camera?.behavior||'composed';$('#camera-enabled').checked=Boolean(camera?.enabled);if(document.activeElement!==$('#camera-stability'))$('#camera-stability').value=tuning?.stability??camera?.stability??.65;if(document.activeElement!==$('#camera-response'))$('#camera-response').value=tuning?.response??camera?.response??.65;const ratio=project().aspect_ratio.split(':').map(Number),source=((a.media?.width||1)*(1-(c.transform.foreground_crop?.left||0)-(c.transform.foreground_crop?.right||0)))/((a.media?.height||1)*(1-(c.transform.foreground_crop?.top||0)-(c.transform.foreground_crop?.bottom||0))),mode=window.NLClipAnimation.resolvedFit(c.transform,source,ratio[0]/ratio[1]);$('#composition-advice').textContent=(c.transform.fit==='auto'?`Auto → ${mode==='blur'?'sfondo sfocato: il crop scarterebbe molto materiale o i volti sono incerti':'riempi: ritaglio ridotto o centratura osservata stabile'}. `:'')+(camera?.advice||'Puoi analizzare il volto oppure regolare fuoco e ritaglio manualmente.');const points=$('#camera-points');if(document.activeElement!==points)points.value=camera?JSON.stringify(camera.points,null,2):'';$('#camera-status').textContent=camera?`${camera.points.length} punti sorgente · ${camera.samples.filter(p=>p.cut).length} possibili stacchi · ${camera.samples.filter(p=>!p.center).length} campioni senza volto. Rivedi il playback; nessun riconoscimento di identità.`:'Nessuna traiettoria. Il fuoco manuale resta statico.';
    const values={start_ms:c.start_ms/1000,trim_in_ms:c.trim_in_ms/1000,duration_ms:c.duration_ms/1000,volume:c.volume,opacity:c.opacity,track_id:c.track_id,label:c.label||a?.name||'',x:(c.transform?.x??.5)*100,y:(c.transform?.y??.5)*100,scale:(c.transform?.scale??1)*100,rotation:c.transform?.rotation??0,focus_x:(c.transform?.focus_x??.5)*100,focus_y:(c.transform?.focus_y??.5)*100};
    for(const k of ['fade_in_ms','fade_out_ms','audio_fade_in_ms','audio_fade_out_ms'])values[k]=(c.transform?.[k]||0)/1000;
    for(const k of ['zoom_start','zoom_end'])values[k]=(c.transform?.[k]??1)*100;
    for(const k of ['pan_x','pan_y'])values[k]=(c.transform?.[k]||0)*100;for(const [k,v]of [['background_blur',.035],['background_brightness',.48],['background_intensity',1]])values[k]=(c.transform?.[k]??v)*100;for(const side of ['left','right','top','bottom'])values['foreground_'+side]=(c.transform?.foreground_crop?.[side]||0)*100;
    const target=$('#inspector-track');target.innerHTML=timeline().tracks.filter(t=>validTrack(c,t)).map(t=>`<option value="${t.id}">${esc(t.name)}</option>`).join('');
    values.fit=c.transform?.fit||'contain';
    document.querySelectorAll('[data-inspect]').forEach(el=>{if(el!==document.activeElement){const value=values[el.dataset.inspect];el.value=typeof value==='number'?Number(value.toFixed(6)):value;}});
  }
  async function commit(tl){pause();s.draft=null;await s.api.commit(tl);}
  async function mutate(action){if(!project())throw Error('Scegli un progetto.');const tl=clone(project().timeline);await action(tl);await commit(tl);}
  async function split(){const c=selected();if(!c)throw Error('Seleziona un clip.');if(track(c.track_id)?.locked)throw Error('Sblocca la traccia.');const offset=Math.round(frame(s.time-c.start_ms));if(offset<1||offset>=c.duration_ms)throw Error('Posiziona il cursore dentro il clip da dividere.');await mutate(tl=>{const item=tl.clips.find(x=>x.id===c.id);const next={...clone(item),id:crypto.randomUUID(),start_ms:item.start_ms+offset,trim_in_ms:item.trim_in_ms+offset,duration_ms:item.duration_ms-offset};item.duration_ms=offset;tl.clips.push(next);});}
  async function remove(ripple=false){if(s.objectSelected)return removeObject();const c=selected();if(!c)return;if(track(c.track_id)?.locked)throw Error('Sblocca la traccia.');await mutate(tl=>{tl.clips=tl.clips.filter(x=>x.id!==c.id);if(ripple)for(const other of tl.clips)if(other.track_id===c.track_id&&other.start_ms>=c.start_ms+c.duration_ms)other.start_ms-=c.duration_ms;});}
  async function duplicate(){const c=selected();if(!c)return;if(track(c.track_id)?.locked)throw Error('Sblocca la traccia.');await mutate(tl=>tl.clips.push({...clone(c),id:crypto.randomUUID(),start_ms:c.start_ms+c.duration_ms}));}
  async function addAsset(id,trackId,at=s.time){const a=asset(id);if(!a)return;const tl=clone(project().timeline);const t=trackId?tl.tracks.find(t=>t.id===trackId):tl.tracks.find(t=>!t.locked&&t.kind===(a.kind==='audio'?'audio':a.kind==='image'?'overlay':'video'));const c={id:crypto.randomUUID(),asset_id:id,track_id:t?.id,start_ms:Math.max(0,Math.round(frame(at))),trim_in_ms:0,duration_ms:a.kind==='image'?5000:(a.media?.duration_ms||5000),label:a.name,volume:1,opacity:1,transform:{...defaults(),fit:a.kind==='video'?'auto':'contain'}};if(!validTrack(c,t))throw Error('Scegli una traccia compatibile e sbloccata.');tl.clips.push(c);s.selected=c.id;await commit(tl);}
  function snap(value,exclude){if(!s.snap)return value;const points=[0,s.time,...timeline().clips.filter(c=>c.id!==exclude).flatMap(c=>[c.start_ms,c.start_ms+c.duration_ms])];let nearest=value,dist=8/s.pps*1000;for(const p of points)if(Math.abs(p-value)<dist){nearest=p;dist=Math.abs(p-value);}return nearest;}
  function position(event){const rect=$('#nle-scroll').getBoundingClientRect();return Math.max(0,(event.clientX-rect.left+$('#nle-scroll').scrollLeft-118)/s.pps*1000);}
  function gestureStart(e){if(e.button!==0||!project()||e.target.closest('button'))return;const viewport=$('#nle-scroll'),bounds=viewport.getBoundingClientRect();if(e.clientX>=bounds.left+viewport.clientLeft+viewport.clientWidth||e.clientY>=bounds.top+viewport.clientTop+viewport.clientHeight)return;const objectBlock=e.target.closest('[data-nle-object]');if(objectBlock){
    const id=objectBlock.dataset.nleObject,edge=e.target.dataset.edge;selectObject(id);if(s.objectSelected!==id)return;
    const o=object(id),lane=trackForObject(o),job=s.api.getJobs().find(j=>j.id===o.job_id);
    if(lane?.locked||['queued','running'].includes(job?.status)||s.objectDirty)return;
    pause();const block=document.querySelector(`[data-nle-object="${id}"]`);
    s.gesture={kind:'object',original:clone(o),next:clone(o),edge,x:e.clientX,block,moved:false};$('#nle-scroll').setPointerCapture(e.pointerId);e.preventDefault();return;
  }const block=e.target.closest('[data-nle-clip]');if(!block){pause();s.time=Math.min(end(),position(e));s.gesture={kind:'seek'};$('#nle-scroll').setPointerCapture(e.pointerId);draw();syncHead();return;}const c=clip(block.dataset.nleClip);select(c.id);if(s.tool==='blade'){s.time=position(e);split().catch(report);return;}if(track(c.track_id)?.locked)return;pause();s.gesture={kind:'clip',original:clone(c),edge:e.target.dataset.edge,x:e.clientX,block,moved:false};s.draft=clone(project().timeline);block.setPointerCapture(e.pointerId);}
  function gestureMove(e){const g=s.gesture;if(!g)return;if(g.kind==='seek'){s.time=Math.min(end(),position(e));draw();syncHead();return;}const dx=e.clientX-g.x;if(!g.moved&&Math.abs(dx)<3)return;g.moved=true;
    if(g.kind==='object'){
      const o=g.original,source=clip(o.source_clip_id),lo=source?.start_ms||0,hi=source?source.start_ms+source.duration_ms:86400000,delta=frame(dx/s.pps*1000);
      if(g.edge==='end')g.next.duration_ms=Math.round(clamp(snap(o.start_ms+o.duration_ms+delta),o.start_ms+34,hi)-o.start_ms);
      else if(g.edge==='start'){g.next.start_ms=Math.round(clamp(snap(o.start_ms+delta),lo,o.start_ms+o.duration_ms-34));g.next.duration_ms=o.start_ms+o.duration_ms-g.next.start_ms;}
      else g.next.start_ms=Math.round(clamp(snap(o.start_ms+delta),lo,hi-o.duration_ms));
      g.block.style.left=`${118+g.next.start_ms/1000*s.pps}px`;g.block.style.width=`${Math.max(3,g.next.duration_ms/1000*s.pps)}px`;g.block.classList.add('dragging');
      $('#object-start').value=(g.next.start_ms/1000).toFixed(3);$('#object-duration').value=(g.next.duration_ms/1000).toFixed(3);return;
    }
    const c=clip(g.original.id),o=g.original,a=asset(c.asset_id),delta=frame(dx/s.pps*1000),minimum=1000/fps();
    if(g.edge==='start'){const shift=clamp(delta,-Math.min(o.trim_in_ms,o.start_ms),o.duration_ms-minimum);c.start_ms=Math.round(o.start_ms+shift);c.trim_in_ms=Math.round(o.trim_in_ms+shift);c.duration_ms=Math.round(o.duration_ms-shift);}
    else if(g.edge==='end'){const limit=a.kind==='image'?86_400_000:(a.media?.duration_ms??o.trim_in_ms+o.duration_ms);const stop=clamp(frame(snap(o.start_ms+o.duration_ms+delta,c.id)),o.start_ms+minimum,o.start_ms+limit-o.trim_in_ms);c.duration_ms=Math.round(stop-o.start_ms);}
    else{c.start_ms=Math.max(0,Math.round(frame(snap(o.start_ms+delta,c.id))));const lane=document.elementFromPoint(e.clientX,e.clientY)?.closest('[data-lane]');if(lane&&validTrack(c,track(lane.dataset.lane)))c.track_id=lane.dataset.lane;}
    g.block.style.left=`${118+c.start_ms/1000*s.pps}px`;g.block.style.width=`${Math.max(3,c.duration_ms/1000*s.pps)}px`;g.block.classList.add('dragging');draw();inspect();
  }
  async function gestureEnd(){const g=s.gesture;if(!g)return;
    if(g.kind==='object'){try{if(g.moved){const pid=project().id;const p=await s.api.api(`/api/projects/${pid}/graphic-objects/${g.original.id}`,'PUT',{start_ms:g.next.start_ms,duration_ms:g.next.duration_ms});if(project()?.id===pid)s.api.acceptProject(p);s.api.notify('Posizione oggetto salvata.');}}finally{s.gesture=null;render();inspect();}return;}
    s.gesture=null;if(g.kind==='clip'){const tl=s.draft;s.draft=null;if(g.moved){try{await commit(tl);}catch(e){render();report(e);}}else{render();}}}
  function media(c,a){if(s.players.has(c.id))return s.players.get(c.id);const pid=project().id,element=document.createElement(a.kind==='audio'?'audio':'video');element.preload='auto';element.playsInline=true;const player={element,gain:null,node:null,message:'Caricamento anteprima…',fallback:false};
    const load=force=>window.MediaPreview.resolve(pid,a,{force,onStatus:message=>{player.message=message;if(!s.playing&&project()?.id===pid)draw();}}).then(url=>{if(s.players.get(c.id)===player){element.src=url;element.load();}}).catch(error=>{player.message=error.message;pause();report(error);});
    element.addEventListener('error',()=>{if(!player.fallback){player.fallback=true;load(true);}else{player.message='Impossibile riprodurre il media. Rigenera l’anteprima dalla sorgente.';pause();report(Error(player.message));}});const ready=()=>{if(element.readyState>=2)player.message='';if(!s.playing)draw();if(s.awaitingPlay&&!s.playing&&project()?.id===pid)play();};element.addEventListener('seeked',ready);element.addEventListener('loadeddata',ready);element.addEventListener('canplay',ready);if(a.kind==='video'&&element.requestVideoFrameCallback){const presented=(_,meta)=>{player.presentedTime=meta.mediaTime*1000;if(!s.playing)draw();if(s.players.get(c.id)===player)player.callback=element.requestVideoFrameCallback(presented);};player.callback=element.requestVideoFrameCallback(presented);}s.players.set(c.id,player);load(false);return player;}
  function connect(player,c){if(!s.audio)return;try{if(!player.node){player.node=s.audio.createMediaElementSource(player.element);player.gain=s.audio.createGain();player.node.connect(player.gain);}const target=window.AudioMixer?.destination(s.audio,c)||s.audio.destination;if(player.destination!==target){player.gain.disconnect();player.gain.connect(target);player.destination=target;}}catch(e){report(e);}}
  function draw(){const canvas=$('#program-canvas');if(!canvas)return;const ctx=canvas.getContext('2d');const tl=timeline();if(!tl){ctx.clearRect(0,0,canvas.width,canvas.height);return;}const ratio=project().aspect_ratio.split(':').map(Number);const fullWidth=1080*ratio[0]/ratio[1],factor=Math.min(1,1280/Math.max(fullWidth,1080));const width=Math.round(fullWidth*factor),height=Math.round(1080*factor);if(canvas.width!==width||canvas.height!==height){canvas.width=width;canvas.height=height;}const well=$('.program-well'),dual=well.classList.contains('dual-monitor'),availableWidth=dual?(well.clientWidth-20)/2:well.clientWidth,availableHeight=well.clientHeight-(dual?38:0),fit=Math.min((availableWidth||640)/width,(availableHeight||340)/height),frameNode=$('#program-frame');frameNode.style.width=`${Math.round(width*fit)}px`;frameNode.style.height=`${Math.round(height*fit)}px`;ctx.fillStyle='#000';ctx.fillRect(0,0,width,height);
    window.AudioMasterUI?.sync(s.time,s.playing);window.AudioMixer?.sync(s.audio,s.time,s.playing);const active=new Set();let loading='';
    for(const t of tl.tracks){if(!t.visible)continue;for(const c of [...tl.clips].filter(c=>c.track_id===t.id).sort((a,b)=>a.start_ms-b.start_ms)){
      if(s.time<c.start_ms||s.time>=c.start_ms+c.duration_ms)continue;const a=asset(c.asset_id);if(!a)continue;let image,visualTime=s.time;
      if(a.kind==='image'){if(!s.images.has(a.id)){const i=new Image();i.src=sourceUrl(a.id);i.onload=()=>draw();s.images.set(a.id,i);}const i=s.images.get(a.id);if(i.complete&&i.naturalWidth)image=i;}
      else if(['video','audio'].includes(a.kind)){active.add(c.id);const player=media(c,a),el=player.element;const target=(c.trim_in_ms+s.time-c.start_ms)/1000;if(el.readyState&&Math.abs(el.currentTime-target)>(s.playing?0.18:0.015)){try{el.currentTime=target;}catch{}}
        if(el.readyState<2)loading=player.message||'Caricamento anteprima…';connect(player,c);const gain=window.AudioMasterUI?.required()?0:c.volume*window.NLClipAnimation.pose(c,s.time).audio;if(player.gain){el.volume=1;player.gain.gain.value=gain;}else el.volume=clamp(gain,0,1);
        if(s.playing&&el.paused)el.play().catch(()=>{});if(!s.playing&&!el.paused)el.pause();if(a.kind==='video'&&el.readyState>=2){image=el;visualTime=c.start_ms+(player.presentedTime??el.currentTime*1000)-c.trim_in_ms;}
      }
      if(image&&t.kind!=='audio'){const iw=image.videoWidth||image.naturalWidth,ih=image.videoHeight||image.naturalHeight;if(!iw||!ih)continue;const tr={...defaults(),...c.transform},pose=window.NLClipAnimation.pose(c,s.time),crop=tr.foreground_crop||{left:0,right:0,top:0,bottom:0},sw=iw*(1-crop.left-crop.right),sh=ih*(1-crop.top-crop.bottom),mode=window.NLClipAnimation.resolvedFit(tr,sw/sh,width/height),fit=Math.min(width/sw,height/sh)*tr.scale*pose.zoom;
        if(mode==='blur')window.NLClipAnimation.background(ctx,image,width,height,tr,c.opacity*pose.alpha,a.kind==='video'?visualTime:0);
        ctx.save();ctx.translate(width*pose.x,height*pose.y);ctx.rotate(tr.rotation*Math.PI/180);ctx.globalAlpha=c.opacity*pose.alpha;
        let boxW=sw*fit,boxH=sh*fit;
        if(mode==='cover'){const ratio=Math.max(width/sw,height/sh),cw=width/ratio,ch=height/ratio,center=window.NLClipAnimation.camera(c,visualTime),cx=center?clamp(iw*center.x-cw/2,iw*crop.left,iw*crop.left+sw-cw):iw*crop.left+(sw-cw)*tr.focus_x,cy=center?clamp(ih*center.y-ch/2,ih*crop.top,ih*crop.top+sh-ch):ih*crop.top+(sh-ch)*tr.focus_y;boxW=width*tr.scale*pose.zoom;boxH=height*tr.scale*pose.zoom;ctx.drawImage(image,cx,cy,cw,ch,-boxW/2,-boxH/2,boxW,boxH);}else ctx.drawImage(image,iw*crop.left,ih*crop.top,sw,sh,-boxW/2,-boxH/2,boxW,boxH);
        if(c.id===s.selected&&!s.playing){ctx.globalAlpha=1;ctx.strokeStyle='#dfb4ff';ctx.lineWidth=2;ctx.strokeRect(-boxW/2,-boxH/2,boxW,boxH);}ctx.restore();}
    }}
    window.MotionStudio?.draw(ctx,project(),s.time,s.playing);window.SubtitleStudio?.draw(ctx,project(),s.time);window.MotionStudio?.mirror(canvas,s.time,s.playing);window.SocialStudio?.mirror(canvas);s.players.forEach((p,id)=>{if(!active.has(id)&&!p.element.paused)p.element.pause();});const message=$('#program-status');if(message){message.hidden=!loading&&Boolean(tl.clips.length);message.textContent=loading||'Timeline vuota · Apri un media dalla libreria o trascinalo su una traccia.';}syncHead();
  }
  function tick(now){if(!s.playing)return;const master=window.AudioMasterUI?.required()?window.AudioMasterUI.playback():null;s.time=Math.min(end(),master&&!master.paused?master.current_ms:now-s.origin);draw();if(s.time>=end()){pause();return;}s.raf=requestAnimationFrame(tick);}
  function pause(){s.playRevision=(s.playRevision||0)+1;window.AudioMasterUI?.pause();window.AudioMixer?.pause();s.playing=false;s.awaitingPlay=false;cancelAnimationFrame(s.raf);s.players.forEach(p=>p.element.pause());window.MotionStudio?.pause();if($('#program-play'))$('#program-play').textContent='▶';}
  async function play(){if(s.preparingAudio)return;if(!project()||!end())return;if(s.playing){pause();return;}if(window.AudioMasterUI?.required()){const token=s.playRevision||0;s.preparingAudio=true;$('#program-play').textContent='…';try{await window.AudioMasterUI.ensure();if(token!==(s.playRevision||0))return;}catch(e){pause();report(e);return;}finally{s.preparingAudio=false;}}s.api.showProgram?.();$('#preview')?.pause();if(!s.audio){try{s.audio=new(window.AudioContext||window.webkitAudioContext)();}catch{}}s.audio?.resume().catch(()=>{});if(s.time>=end())s.time=0;draw();const waiting=timeline().clips.some(c=>track(c.track_id)?.visible&&s.time>=c.start_ms&&s.time<c.start_ms+c.duration_ms&&['video','audio'].includes(asset(c.asset_id)?.kind)&&(s.players.get(c.id)?.element.readyState||0)<2);if(waiting){s.awaitingPlay=true;$('#program-play').textContent='…';return;}s.awaitingPlay=false;if(window.AudioMasterUI?.required()){try{await window.AudioMasterUI.start(s.time);}catch(e){pause();report(e);return;}}s.playing=true;s.origin=performance.now()-s.time;$('#program-play').textContent='❚❚';draw();s.raf=requestAnimationFrame(tick);}
  function seek(ms){pause();s.api?.showProgram?.();s.time=clamp(ms,0,end());draw();followHead(true);}
  function zoom(value){const scroll=$('#nle-scroll'),anchor=s.time/1000*s.pps-scroll.scrollLeft;s.pps=clamp(value,2,300);$('#nle-zoom').value=s.pps;render();scroll.scrollLeft=Math.max(0,s.time/1000*s.pps-anchor);followHead();}
  function bind(){const listen=(id,event,fn)=>$(id).addEventListener(event,e=>{try{Promise.resolve(fn(e)).catch(report);}catch(error){report(error);}});
    $('#clip-inspector').insertAdjacentHTML('beforeend','<details class="clip-animation"><summary>Camera fluida · auto reframe CPU</summary><p class="muted">Seleziona un video, analizza e adotta la proposta. Campioni 4Hz, traiettoria continua con zona morta e velocità controllata. Assenza volto: mantiene la camera; possibili stacchi: reset senza attraversamento. Più soggetti: revisione necessaria, non riconoscimento di identità. Massimo180s per clip.</p><label>Comportamento<select id="camera-behavior"><option value="composed">Camera composta · stabile (consigliato)</option><option value="fixed">Centratura fissa per inquadratura</option><option value="legacy">Inseguimento precedente · confronto</option></select></label><label>Stabilizzazione<input id="camera-stability" type="range" min="0" max="1" step=".05" value=".65"></label><label>Reattività<input id="camera-response" type="range" min="0" max="1" step=".05" value=".65"></label><button id="camera-analyze" type="button" class="secondary">Analizza / ricalcola dai campioni in cache</button><button id="camera-adopt" type="button" class="secondary" hidden>Adotta proposta revisionabile</button><label><input id="camera-enabled" type="checkbox"> Usa traiettoria con Riempi</label><p id="camera-status" class="muted" role="status"></p><details><summary>Punti sorgente modificabili · avanzato</summary><textarea id="camera-points" rows="6" spellcheck="false" aria-label="Punti camera JSON"></textarea><p class="muted">source_ms, x/y normalizzati, vx/vy per secondo, cut per uno stacco. Applica con l’inspector. Raw rilevamenti conservati; disattiva per tornare al fuoco manuale.</p></details></details>');
    let proposal=null;
    for(const id of ['camera-stability','camera-response','camera-behavior'])listen('#'+id,'input',()=>{const c=selected();if(c){s.cameraTuning||={};s.cameraTuning[c.id]={stability:Number($('#camera-stability').value),response:Number($('#camera-response').value),behavior:$('#camera-behavior').value};}});
    listen('#camera-analyze','click',async()=>{const c=selected();if(!c)throw Error('Seleziona un clip video.');proposal=null;$('#camera-adopt').hidden=true;const pid=project().id,button=$('#camera-analyze');button.disabled=true;try{let job=await s.api.api(`/api/projects/${pid}/timeline/${c.id}/reframe`,'POST',{expected_updated_at:project().updated_at,stability:Number($('#camera-stability').value),response:Number($('#camera-response').value),behavior:$('#camera-behavior').value});while(['queued','running'].includes(job.status)){await new Promise(r=>setTimeout(r,500));job=await s.api.api('/api/jobs/'+job.id);if(project()?.id!==pid)return;$('#camera-status').textContent=job.message;}if(job.status!=='succeeded')throw Error(job.message);proposal={pid,...job.result};$('#camera-adopt').hidden=false;$('#camera-status').textContent=`Proposta pronta · ${proposal.camera_path.cache_hit?'campioni riusati, nessun nuovo decoding':'analisi CPU completata'}. Adotta soltanto dopo aver controllato i campioni e il playback.`;}finally{button.disabled=false;}});
    listen('#camera-adopt','click',async()=>{const c=selected();if(!proposal||proposal.pid!==project()?.id||c?.id!==proposal.clip_id||c.asset_id!==proposal.asset_id||c.trim_in_ms!==proposal.trim_in_ms||c.duration_ms!==proposal.duration_ms)throw Error('Proposta non corrispondente al clip selezionato: analizza di nuovo.');const accepted=proposal;proposal=null;$('#camera-adopt').hidden=true;await mutate(tl=>{const tr=tl.clips.find(i=>i.id===c.id).transform;tr.camera_path=clone(accepted.camera_path);const preferBlur=accepted.camera_path.preferred_fit==='blur';tr.fit=preferBlur?'blur':(accepted.camera_path.behavior&&accepted.camera_path.behavior!=='legacy'?'auto':'cover');if(preferBlur){tr.background_blur=Math.max(.06,Number(tr.background_blur)||0);tr.background_brightness=Math.min(.4,Number(tr.background_brightness)||.48);tr.background_intensity=Number(tr.background_intensity)||1;}if($('#camera-status'))$('#camera-status').textContent=(preferBlur?'Soggetto non stabile: 16:9 su sfondo sfocato. ':'Soggetto agganciato: crop sicuro. ')+(accepted.camera_path.advice||'');});});
    $('#clip-inspector').insertAdjacentHTML('beforeend','<details class="clip-animation"><summary>Movimento fotografico e dissolvenze</summary><p class="muted">Zoom proporzionale, nessuna modifica al volto. Il pan può lasciare bordi vuoti: controlla il monitor. Fade zero conserva l’eventuale animazione predefinita.</p>'+[['fade_in_ms','Ingresso visual (s)',0,60,.05],['fade_out_ms','Uscita visual (s)',0,60,.05],['audio_fade_in_ms','Ingresso audio (s)',0,60,.05],['audio_fade_out_ms','Uscita audio (s)',0,60,.05],['zoom_start','Zoom iniziale (%)',100,150,1],['zoom_end','Zoom finale (%)',100,150,1],['pan_x','Pan orizzontale (%)',-25,25,.1],['pan_y','Pan verticale (%)',-25,25,.1]].map(([k,l,min,max,step])=>`<label>${l}<input type="number" data-inspect="${k}" min="${min}" max="${max}" step="${step}"></label>`).join('')+'</details>');
    $('#clip-inspector').insertAdjacentHTML('beforeend','<label>Adattamento<select id="clip-fit" data-inspect="fit"><option value="contain">Contieni · intero fotogramma</option><option value="cover">Riempi · crop intelligente/manuale</option><option value="blur">Adatta con sfondo video sfocato</option><option value="auto">Automatico · scelta prudente e modificabile</option></select></label><p id="composition-advice" class="muted"></p><details><summary>Sfondo sincronizzato / ritaglio foreground</summary><p class="muted">Lo sfondo usa lo stesso frame originale. Scala e posizione agiscono solo sul primo piano; i ritagli sono non distruttivi.</p>'+[['background_blur','Sfocatura (% larghezza)',0,8,.1],['background_brightness','Luminosità sfondo (%)',0,150,1],['background_intensity','Intensità sfondo (%)',0,100,1],['foreground_left','Ritaglia a sinistra (%)',0,95,.5],['foreground_right','Ritaglia a destra (%)',0,95,.5],['foreground_top','Ritaglia sopra (%)',0,95,.5],['foreground_bottom','Ritaglia sotto (%)',0,95,.5]].map(([k,l,min,max,step])=>`<label>${l}<input type="number" data-inspect="${k}" min="${min}" max="${max}" step="${step}"></label>`).join('')+'</details><label>Fuoco crop X (%)<input data-inspect="focus_x" type="number" min="0" max="100" step="1" value="50"></label><label>Fuoco crop Y (%)<input data-inspect="focus_y" type="number" min="0" max="100" step="1" value="50"></label>');
    listen('#program-play','click',play);listen('#program-back','click',()=>seek(s.time-1000/fps()));listen('#program-forward','click',()=>seek(s.time+1000/fps()));listen('#program-seek','change',e=>seek(Number(e.target.value)*1000));
    listen('#nle-add-track','click',()=>mutate(tl=>{const kind=$('#nle-track-kind').value,count=tl.tracks.filter(t=>t.kind===kind).length+1;tl.tracks.push({id:crypto.randomUUID(),name:({video:'Video',overlay:'Grafiche',audio:'Audio'})[kind]+' '+count,kind,visible:true,locked:false});}));
    listen('#open-clip-object','click',e=>selectObject(e.currentTarget.dataset.object));
    listen('#nle-new-object','click',newObject);listen('#nle-split','click',split);listen('#nle-remove','click',()=>remove());listen('#nle-ripple','click',()=>remove(true));listen('#nle-duplicate','click',duplicate);listen('#nle-select-tool','click',()=>{s.tool='select';render();});listen('#nle-blade-tool','click',()=>{s.tool='blade';render();});
    listen('#nle-zoom','input',e=>zoom(Number(e.target.value)));listen('#nle-fit','click',()=>zoom(Math.max(2,($('#nle-scroll').clientWidth-155)/Math.max(1,end()/1000))));listen('#nle-snap','change',e=>{s.snap=e.target.checked;});
    $('#nle-scroll').addEventListener('pointerdown',gestureStart);$('#nle-scroll').addEventListener('pointermove',gestureMove);$('#nle-scroll').addEventListener('pointerup',()=>gestureEnd().catch(report));$('#nle-scroll').addEventListener('pointercancel',()=>{s.gesture=null;s.draft=null;render();draw();});
    listen('#nle-scroll','click',async e=>{const visible=e.target.closest('[data-lane-visible]'),locked=e.target.closest('[data-lane-lock]');if(visible)await mutate(tl=>{const t=tl.tracks.find(t=>t.id===visible.dataset.laneVisible);t.visible=!t.visible;});if(locked)await mutate(tl=>{const t=tl.tracks.find(t=>t.id===locked.dataset.laneLock);t.locked=!t.locked;});});
    $('#assets').addEventListener('dragstart',e=>{const item=e.target.closest('[data-library-asset]');if(item){e.dataTransfer.setData('application/x-nl-asset',item.dataset.libraryAsset);e.dataTransfer.effectAllowed='copy';}});
    $('#nle-scroll').addEventListener('dragover',e=>{if(e.dataTransfer.types.includes('application/x-nl-asset')){e.preventDefault();e.dataTransfer.dropEffect='copy';}});
    listen('#nle-scroll','drop',async e=>{e.preventDefault();const lane=e.target.closest('[data-lane]');const id=e.dataTransfer.getData('application/x-nl-asset');if(id&&lane)await addAsset(id,lane.dataset.lane,position(e));});
    listen('#inspector-apply','click',async()=>{const c=selected();if(!c)return;await mutate(tl=>{const n=tl.clips.find(x=>x.id===c.id);n.transform={...defaults(),...n.transform};if(n.transform.camera_path){n.transform.camera_path.enabled=$('#camera-enabled').checked;n.transform.camera_path.points=JSON.parse($('#camera-points').value);}document.querySelectorAll('[data-inspect]').forEach(el=>{const k=el.dataset.inspect;if(k==='fit'){n.transform.fit=el.value;}else if(k.startsWith('background_')){n.transform[k]=Number(el.value)/100;}else if(k.startsWith('foreground_')){n.transform.foreground_crop||={left:0,right:0,top:0,bottom:0};n.transform.foreground_crop[k.slice(11)]=Number(el.value)/100;}else if(['fade_in_ms','fade_out_ms','audio_fade_in_ms','audio_fade_out_ms'].includes(k)){n.transform[k]=Math.round(Number(el.value)*1000);}else if(['x','y','scale','rotation','focus_x','focus_y','zoom_start','zoom_end','pan_x','pan_y'].includes(k)){n.transform[k]=Number(el.value)/(k==='rotation'?1:100);}else n[k]=['track_id','label'].includes(k)?el.value:Math.round(Number(el.value)*(k.endsWith('_ms')?1000:10000))/(k.endsWith('_ms')?1:10000);});if(window.__nlSocialBlur){n.transform.background_blur=Math.max(.06,Number(n.transform.background_blur)||0);n.transform.background_brightness=Math.min(.4,Number(n.transform.background_brightness)||.48);n.transform.background_intensity=Number(n.transform.background_intensity)||1;window.__nlSocialBlur=false;}});});
    listen('#inspector-reset','click',()=>{const c=selected();if(c)return mutate(tl=>{tl.clips.find(x=>x.id===c.id).transform=defaults();});});
    listen('#inspector-graphic','click',()=>s.api.switchTab('graphics'));listen('#inspector-generate','click',()=>s.api.switchTab('workflows'));
    $('#program-canvas').addEventListener('pointerdown',e=>{const c=selected();if(!c||track(c.track_id)?.locked)return;pause();s.gesture={kind:'transform',x:e.clientX,y:e.clientY,original:clone(c)};s.draft=clone(project().timeline);e.target.setPointerCapture(e.pointerId);});
    $('#program-canvas').addEventListener('pointermove',e=>{const g=s.gesture;if(g?.kind!=='transform')return;const c=clip(g.original.id),rect=e.target.getBoundingClientRect();c.transform={...defaults(),...g.original.transform};c.transform.x+=(e.clientX-g.x)/rect.width;c.transform.y+=(e.clientY-g.y)/rect.height;g.moved=true;draw();inspect();});
    listen('#program-canvas','pointerup',async()=>{const g=s.gesture;if(g?.kind!=='transform')return;s.gesture=null;const tl=s.draft;s.draft=null;if(g.moved)await commit(tl);});
    document.addEventListener('keydown',e=>{if(e.defaultPrevented||!$('#tab-studio')?.classList.contains('active')||e.target.closest('input,textarea,select,[contenteditable=true],[role=separator]')||e.target.closest('dialog'))return;let action;const modifier=e.ctrlKey||e.metaKey;if(e.code==='Space')action=play;if(e.key==='ArrowLeft')action=()=>seek(s.time-(e.shiftKey?10:1)*1000/fps());if(e.key==='ArrowRight')action=()=>seek(s.time+(e.shiftKey?10:1)*1000/fps());if(!modifier&&e.key.toLowerCase()==='s'||modifier&&e.key.toLowerCase()==='k')action=split;if(!modifier&&['v','c'].includes(e.key.toLowerCase()))action=()=>{s.tool=e.key.toLowerCase()==='c'?'blade':'select';render();};if(e.key==='Delete'||e.key==='Backspace')action=()=>remove(e.shiftKey);if(modifier&&e.key.toLowerCase()==='z')action=()=>$(e.shiftKey?'#redo':'#undo').click();if(action){e.preventDefault();Promise.resolve().then(action).catch(report);}});
    listen('#object-save','click',async()=>{await saveObject();s.api.notify('Oggetto salvato.');});
    listen('#object-describe','click',()=>objectAction('describe'));listen('#object-generate','click',()=>objectAction('generate'));listen('#object-approve','click',()=>objectAction('approve'));
    $('#object-inspector').addEventListener('input',()=>{s.objectDirty=true;});
    $('#program-canvas').addEventListener('pointercancel',()=>{s.gesture=null;s.draft=null;draw();inspect();});
    let lastTimelineWidth=-1;new ResizeObserver(()=>{const width=$('#nle-scroll').clientWidth;if(project()&&width>0&&width!==lastTimelineWidth&&!s.gesture&&!window.SubtitleStudio?.isEditing()){lastTimelineWidth=width;render();}}).observe($('#nle-scroll'));
    new ResizeObserver(()=>{if(project())draw();}).observe($('.program-well'));
  }
  return{configure,update,select,selectObject,newObject,getObject:()=>object(s.objectSelected),seek,addAsset,pause,getTime:()=>s.time,isEditing:()=>Boolean(s.gesture)||s.objectDirty,draw,refreshControls:inspect,unlinkAudio,relinkAudio};
})();

// Resizable desktop workspace. Preferences affect the UI only, never project data.
(()=>{
 const grid=document.querySelector('.edit-grid');if(!grid)return;
 const studio=document.querySelector('#tab-studio'),key='nl-suite.workspace.v1';
 // Keep room for an inspectable portrait monitor, not a postage stamp after
 // toolbars/transport/export status have consumed the old 300px panel.
 const defaults=()=>({left:230,right:265,height:Math.max(240,(innerHeight-170)*.56)});
 let layout=defaults(),gesture=null;
 try{const saved=JSON.parse(localStorage.getItem(key));if(saved&&['left','right','height'].every(k=>Number.isFinite(saved[k])))layout=saved;}catch{}
 const toolbar=document.createElement('div');toolbar.className='workspace-controls';
 toolbar.innerHTML='<span>Spazio di lavoro</span><button type="button" class="secondary" data-workspace="edit">Montaggio</button><button type="button" class="secondary" data-workspace="graphics">Grafiche e proprietà</button><button type="button" class="secondary" data-workspace="reset">Ripristina pannelli</button><small>V: selezione · C: lama · Ctrl+K: dividi al cursore · trascina i separatori</small>';
 grid.before(toolbar);
 const handles={};
 for(const [field,label,orientation]of [['left','Larghezza libreria','vertical'],['right','Larghezza controlli','vertical'],['height','Altezza monitor e pannelli','horizontal']]){
  const handle=document.createElement('div');handle.className='workspace-divider '+field;handle.tabIndex=0;
  handle.setAttribute('role','separator');handle.setAttribute('aria-label',label);handle.setAttribute('aria-orientation',orientation);handle.dataset.workspaceResize=field;
  handle.title=label+' · trascina o usa le frecce · doppio clic per ripristinare';
  if(field==='height')grid.after(handle);else grid.append(handle);handles[field]=handle;
  handle.addEventListener('pointerdown',e=>{if(e.button!==0)return;gesture={field,start:e.clientX,top:e.clientY,before:{...layout,height:grid.getBoundingClientRect().height}};handle.setPointerCapture(e.pointerId);e.preventDefault();});
  handle.addEventListener('pointermove',e=>{if(gesture?.field!==field)return;const delta=field==='height'?e.clientY-gesture.top:(e.clientX-gesture.start)*(field==='right'?-1:1);layout[field]=gesture.before[field]+delta;apply();});
  handle.addEventListener('pointerup',()=>{if(!gesture)return;gesture=null;save();});
  handle.addEventListener('pointercancel',()=>{if(!gesture)return;layout=gesture.before;gesture=null;apply();});
  handle.addEventListener('dblclick',()=>{layout[field]=defaults()[field];apply();save();});
  handle.addEventListener('keydown',e=>{if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home'].includes(e.key))return;e.preventDefault();e.stopPropagation();if(field==='height')layout.height=grid.getBoundingClientRect().height;layout[field]=e.key==='Home'?defaults()[field]:layout[field]+(['ArrowLeft','ArrowUp'].includes(e.key)?-1:1)*(e.shiftKey?50:10)*(field==='right'?-1:1);apply();save();});
 }
 function apply(){
  if(!studio.classList.contains('active'))return;
  const width=grid.clientWidth||1000,maxSide=Math.max(170,(width-280-24)/2);
  layout.left=Math.max(170,Math.min(maxSide,layout.left));layout.right=Math.max(170,Math.min(maxSide,layout.right));
  const available=Math.max(390,innerHeight-studio.getBoundingClientRect().top-10),toolbarHeight=toolbar.getBoundingClientRect().height;
  studio.style.setProperty('--studio-available',available+'px');
  const height=Math.max(160,Math.min(available-toolbarHeight-(innerHeight<720?200:270),layout.height));
  studio.style.setProperty('--workspace-height',height+'px');
  grid.style.setProperty('--workspace-left',layout.left+'px');grid.style.setProperty('--workspace-right',layout.right+'px');grid.style.setProperty('--workspace-height',height+'px');
  for(const field of Object.keys(handles))handles[field].setAttribute('aria-valuenow',Math.round(field==='height'?height:layout[field]));
 }
 function save(){try{localStorage.setItem(key,JSON.stringify(layout));}catch{}}
 toolbar.addEventListener('click',e=>{const mode=e.target.dataset.workspace;if(!mode)return;layout=mode==='graphics'?{left:210,right:360,height:Math.max(400,innerHeight-490)}:defaults();apply();save();if(mode==='graphics')document.querySelector('.inspector').scrollTop=0;});
 new ResizeObserver(()=>{if(studio.classList.contains('active')&&innerWidth>1020)apply();}).observe(grid);
 new ResizeObserver(()=>{if(innerWidth>1020)apply();}).observe(document.querySelector('#notice'));
 window.addEventListener('resize',()=>{if(innerWidth>1020)apply();draw();});
 apply();
})();
