'use strict';
window.JobUI=(()=>{
 const label=status=>({queued:'In coda',running:'In corso',succeeded:'Completato',failed:'Non riuscito'}[status]||status);
 let initialized=false;const keys=new Map();
 $('#jobs').before(Object.assign(document.createElement('div'),{id:'job-actions',className:'actions'}));
 $('#job-actions').innerHTML='<button id="archive-jobs" class="secondary">Archivia lavori conclusi</button><button id="restore-jobs" class="secondary">Ripristina storico</button><label class="check"><input type="checkbox" id="jobs-show-archived">Mostra archiviati</label><button id="enable-job-notifications" class="secondary">Attiva notifiche desktop</button>';
 for(const id of ['tab-workflows','tab-graphics']){const box=document.createElement('section');box.className='panel project-activity';box.dataset.activity=id;$('#'+id).prepend(box);}
 const toast=document.createElement('div');toast.id='job-notifications';toast.setAttribute('aria-live','polite');document.body.append(toast);
 const status=document.createElement('div');status.id='project-task-status';status.setAttribute('role','status');$('#notice').after(status);
 on('#archive-jobs','click',async()=>{if(!confirm('Archivia i lavori conclusi di tutti i progetti? Nessun media verrà cancellato e i lavori attivi proseguiranno.'))return;const r=await api('/api/jobs/archive','POST',{project_id:null,archived:true});await poll();notice(`${r.changed} lavori archiviati. Nessun file eliminato.`);});
 on('#restore-jobs','click',async()=>{await api('/api/jobs/archive','POST',{project_id:null,archived:false});await poll();notice('Storico ripristinato.');});
 on('#jobs-show-archived','change',renderJobs);
 document.addEventListener('click',async e=>{const id=e.target.closest('[data-recover-comfy]')?.dataset.recoverComfy;if(!id)return;try{e.target.disabled=true;await api('/api/jobs/'+id+'/recover-comfy','POST');await poll();notice('Recupero del prompt originale: nessuna nuova generazione inviata.');}catch(error){e.target.disabled=false;notice(error.message,true);}});
 on('#enable-job-notifications','click',async()=>{if(!('Notification'in window))throw Error('Notifiche desktop non disponibili nel browser. Gli avvisi interni restano attivi.');const result=await Notification.requestPermission();notice(result==='granted'?'Notifiche desktop attivate.':'Restano attivi gli avvisi nell’app.');});
 function transitions(next,previous){if(!initialized){initialized=true;return;}const old=new Map(previous.map(j=>[j.id,j.status]));
  for(const j of next)if(['succeeded','failed'].includes(j.status)&&old.get(j.id)!==j.status){
   const el=document.createElement('div');el.className='job-toast '+(j.status==='failed'?'bad':'ok');
   const p=document.createElement('p');p.textContent=label(j.status)+' · '+j.message;el.append(p);
   const button=document.createElement('button');button.textContent='Chiudi';button.onclick=()=>el.remove();el.append(button);toast.prepend(el);while(toast.children.length>2)toast.lastChild.remove();setTimeout(()=>el.remove(),j.status==='failed'?15000:8000);
   if('Notification'in window&&Notification.permission==='granted'&&document.hidden)new Notification('NL Video Suite · '+label(j.status),{body:j.message,tag:j.id});
  }
 }
 function render(){
  const own=state.jobs.filter(j=>j.project_id===state.project?.id),active=own.filter(j=>['queued','running'].includes(j.status));
  $('#project-task-status').textContent=active.length?`${active.length} elaborazioni attive nel progetto · ${active[0].message}`:'';
  const relevant=own.filter(j=>['submit_comfy','generate_story','produce_narrative'].includes(j.kind)&&(!j.archived||['running','queued'].includes(j.status))).slice(0,8);
  const html='<h3>Attività del progetto</h3>'+(relevant.map(j=>{
   const seconds=Math.max(0,Math.round(((j.completed_at?Date.parse(j.completed_at):Date.now())-Date.parse(j.started_at||j.created_at))/1000));
   const ids=j.result?._nl_suite?.asset_ids||[];
   return `<div class="job"><strong class="${j.status==='failed'?'bad':''}">${label(j.status)}</strong> · ${Math.floor(seconds/60)}m ${seconds%60}s<p>${escapeHTML(j.message)}</p>${ids.map(id=>{const a=state.project?.assets.find(a=>a.id===id);return a?`<a href="/api/projects/${state.project.id}/assets/${a.id}/file" target="_blank" rel="noopener">Apri ${escapeHTML(a.name)} · ${escapeHTML(a.kind)}</a> `:'';}).join('')}</div>`;
  }).join('')||'<p>Nessuna elaborazione avviata per il progetto selezionato.</p>')+'<p class="muted">Lo stato descrive fasi reali, non percentuali simulate. Completato significa media importato; l’approvazione editoriale resta separata.</p>';
  for(const box of document.querySelectorAll('[data-activity]')){if(keys.get(box)!==html){const wasOpen=box.querySelector('details')?.open??false;box.innerHTML='<details><summary>Attività e risultati del progetto'+(active.length?' · '+active.length+' in corso':'')+'</summary>'+html+'</details>';box.querySelector('details').open=wasOpen;keys.set(box,html);}}
 }
 const recoveryButton=j=>j.kind==='submit_comfy'&&j.status==='failed'&&j.result?._nl_suite?.prompt_id?`<button data-recover-comfy="${j.id}" class="secondary">Recupera prompt originale · senza rigenerare</button>`:'';
 return{label,render,transitions,recoveryButton};
})();
