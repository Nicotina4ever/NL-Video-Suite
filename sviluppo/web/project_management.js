'use strict';
window.ProjectManagement=(()=>{
 const $=q=>document.querySelector(q),esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
 let h,project=null,plan=null,busy=false,dialog;
 const bytes=n=>n>=2**30?(n/2**30).toFixed(2)+' GiB':n>=2**20?(n/2**20).toFixed(1)+' MiB':n.toLocaleString('it-IT')+' byte';
 function configure(hooks){
  h=hooks;const heading=$('#tab-projects h2'),bar=document.createElement('div');bar.className='panelhead';heading.before(bar);bar.append(heading);
  const open=document.createElement('button');open.id='project-delete-open';open.type='button';open.className='secondary';open.textContent='Elimina progetto…';open.disabled=true;bar.append(open);open.onclick=()=>show().catch(error);
  dialog=document.createElement('dialog');dialog.id='project-delete-dialog';dialog.setAttribute('aria-labelledby','project-delete-heading');dialog.style.cssText='width:min(700px,94vw);max-height:90vh;overflow:auto';
  dialog.innerHTML='<div class="panelhead"><h2 id="project-delete-heading">Elimina progetto</h2><button id="project-delete-close" class="secondary">Annulla</button></div><p id="project-delete-title"></p><div id="project-delete-summary" aria-live="polite"></div><p id="project-delete-error" class="bad" role="alert"></p><div class="actions"><button id="project-delete-jobs" class="secondary" hidden>Apri Lavori</button><button id="project-delete-confirm" disabled>Elimina definitivamente questo progetto</button></div>';
  document.body.append(dialog);$('#project-delete-confirm').style.background='#a62e57';$('#project-delete-close').onclick=()=>dialog.close();$('#project-delete-jobs').onclick=()=>{dialog.close();h.switchTab('jobs');};$('#project-delete-confirm').onclick=()=>remove().catch(error);dialog.addEventListener('cancel',e=>{if(busy)e.preventDefault();});
 }
 function update(p){const b=$('#project-delete-open');if(b){b.disabled=!p;b.title=p?'Elimina «'+p.title+'» e i suoi file interni':'Scegli un progetto prima di eliminarlo';}}
 function error(e){$('#project-delete-error').textContent=e.message||String(e);}
 async function show(){project=h.project();if(!project)return;plan=null;$('#project-delete-title').textContent='Progetto selezionato: '+project.title;$('#project-delete-error').textContent='';$('#project-delete-summary').textContent='Conteggio dei file del progetto…';$('#project-delete-confirm').disabled=true;dialog.showModal();await preview();$('#project-delete-close').focus();}
 async function preview(){
  const id=project.id,next=await h.api('/api/projects/'+id+'/delete-preview','POST',{});if(project?.id!==id||!dialog.open)return;plan=next;
  $('#project-delete-summary').innerHTML=`<p><strong>${next.files} file · ${bytes(next.bytes)}</strong> di riferimenti interni.</p><ul>${next.categories.map(c=>`<li>${esc(c.name)}: ${c.files} file · ${bytes(c.bytes)}</li>`).join('')}</ul><p>${esc(next.note)}</p>${next.pending?'<p class="bad">La rimozione precedente è incompleta. Puoi riprovare sui soli file rimasti.</p>':''}${next.blocked.map(s=>`<p class="bad">${esc(s)}</p>`).join('')}`;
  $('#project-delete-confirm').disabled=!next.can_delete;$('#project-delete-confirm').textContent=next.pending?'Riprova eliminazione':'Elimina definitivamente questo progetto';$('#project-delete-jobs').hidden=!next.blocked.some(s=>/lavor/i.test(s));
 }
 async function remove(){
  if(busy||!plan?.can_delete)return;busy=true;$('#project-delete-confirm').disabled=true;$('#project-delete-close').disabled=true;$('#project-delete-error').textContent='';
  try{
   window.NLE?.pause();for(const media of document.querySelectorAll('video,audio'))media.pause();
   const result=await h.api('/api/projects/'+project.id,'DELETE',{plan_id:plan.plan_id,confirmation:project.title});
   if(result.complete){dialog.close();await h.deleted(project.id);h.notice(`Progetto eliminato: ${result.deleted_files} file interni rimossi. Originali e modelli conservati.`+(result.warnings.length?' '+result.warnings.join(' '):''));}
   else {await preview();error('Rimozione incompleta. Chiudi i player o i programmi che usano questi file e riprova. '+result.remaining.slice(0,3).join(' · '));}
  }catch(e){error(e);await preview().catch(()=>{});}finally{busy=false;$('#project-delete-close').disabled=false;$('#project-delete-confirm').disabled=!plan?.can_delete;}
 }
 return{configure,update};
})();
