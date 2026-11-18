'use strict';
// Same persisted production and job queue as scenes; never a parallel project store.
window.NLNarrativeLibrary=(()=>{
 let api,selected='',key='',projectId='',review=null;
 const labels={source:'Materiale di partenza',character:'Personaggio / character sheet',location:'Ambiente / sfondo',style:'Stile visivo',voice:'Reference vocale'};
 const $=v=>document.querySelector(v),esc=v=>String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
 const active=r=>r?.asset_id||r?.source_asset_id;
 const current=()=>api.s.draft?.references?.find(v=>v.id===selected);
 const url=(id)=>`/api/projects/${api.s.project.id}/assets/${id}/file`;
 function attach(ctx){
  api=ctx;
  $('#tab-story .narrative-main').insertAdjacentHTML('afterbegin',`<section class="panel narrative-library" id="narrative-library"><h2>Materiali, personaggi e ambienti</h2><p>Parti dai tuoi media o crea una libreria coerente prima delle scene. Note e analisi dei materiali selezionati aiutano a scrivere la storia; i file approvati si collegano agli ingressi reali dei workflow.</p><div class="actions"><label class="secondary">Importa media<input id="library-upload" type="file" accept="image/*,video/*,audio/*" multiple></label><button class="secondary" id="library-character">+ Personaggio</button><button class="secondary" id="library-location">+ Ambiente</button><button class="secondary" id="library-source">+ Riferimento</button><button id="library-save">Salva materiali</button></div><div class="narrative-pair"><label>Media già nel progetto<select id="library-existing"></select></label><button class="secondary" id="library-attach">Aggiungi alla libreria</button></div><p id="library-status" class="muted" aria-live="polite"></p><div class="library-layout"><nav id="library-list" aria-label="Riferimenti della produzione"></nav><div id="library-editor"></div></div></section>`);
  $('#library-save').insertAdjacentHTML('afterend','<button id="library-suggest" class="secondary">Proponi personaggi e ambienti dai materiali</button>');
  const listen=(selector,event,fn)=>$(selector).addEventListener(event,e=>{Promise.resolve().then(()=>fn(e)).catch(e=>{api.error(e);api.s.ctx.notice(e.message,true);});});
  listen('#library-suggest','click',()=>api.run(async()=>{await api.save();await api.s.ctx.api(api.endpoint('/references/suggest'),'POST',{});api.s.ctx.notice('Proposta della libreria in coda. Le schede esistenti non vengono sostituite.');}));
  for(const kind of ['character','location','source'])listen('#library-'+kind,'click',()=>add(kind));
  listen('#library-save','click',()=>api.run(api.save));
  listen('#library-attach','click',()=>{const id=$('#library-existing').value,a=api.s.project?.assets.find(a=>a.id===id);if(!a)throw Error('Scegli un media del progetto.');add(a.kind==='audio'?'voice':'source',a);});
  listen('#library-list','click',e=>{const b=e.target.closest('[data-library-select]');if(b){selected=b.dataset.librarySelect;key='';render();}});
  listen('#library-upload','change',()=>api.run(async()=>{
   if(!api.s.project)throw Error('Scegli un progetto.');
   const files=[...$('#library-upload').files],pid=api.s.project.id;
   await api.save();
   for(const file of files){
    $('#library-status').textContent=`Importazione di ${file.name}…`;
    const body=new FormData();body.append('file',file);const res=await fetch(`/api/projects/${pid}/assets/upload`,{method:'POST',body}),job=await res.json();if(!res.ok)throw Error(job.error||'Importazione non riuscita.');
    let done;for(let n=0;n<1200;n++){done=await api.s.ctx.api('/api/jobs/'+job.id);if(['succeeded','failed'].includes(done.status))break;await new Promise(r=>setTimeout(r,500));}
    if(done.status!=='succeeded')throw Error(done.message||'Importazione ancora in corso: consulta la coda.');
    await api.s.ctx.reloadProject();if(pid!==api.s.project?.id)throw Error('Progetto cambiato: il media è nel progetto precedente.');
    const a=api.s.project.assets.find(a=>done.output_paths.some(p=>p.replace(/\\/g,'/').endsWith('/'+a.file_name)));
    if(!a)throw Error('Media importato: ricarica e sceglilo dalla libreria del progetto.');
    add(a.kind==='audio'?'voice':'source',a);await api.save();
   }
   $('#library-upload').value='';$('#library-status').textContent='Materiali copiati nel progetto e salvati nella produzione.';
  }));
  listen('#library-editor','input',e=>{
   const r=current(),f=e.target.dataset.libraryField;if(!r||!f)return;
   r[f]=e.target.type==='checkbox'?e.target.checked:e.target.value;r.approved=false;api.markDirty();
  });
  listen('#library-editor','change',e=>{
   const el=e.target,r=current();if(!r)return;
   if(window.NLReferenceRanges.change(r.workflow,el,api.s.project)){r.approved=false;api.markDirty();key='';if(el.dataset.refMode)render();return;}
   if(el.dataset.libraryField){if(el.dataset.libraryField==='kind'&&r.kind==='voice'){const a=api.s.project.assets.find(a=>a.id===active(r));if(a&&a.kind!=='audio'){r.source_asset_id=null;r.asset_id=null;r.analysis='';}}}
   if(el.dataset.libraryAsset){r[el.dataset.libraryAsset]=el.value||null;if(el.dataset.libraryAsset==='source_asset_id'){r.analysis='';r.asset_id=null;}r.approved=false;}
   if(el.dataset.libraryWorkflow){r.workflow=el.value?{workflow_id:el.value,text_input:'',inputs:{},asset_inputs:{}}:null;r.approved=false;}
   if(el.dataset.libraryText){r.workflow.text_input=el.value;r.approved=false;}
   if(el.dataset.libraryRef){if(r.workflow.asset_ranges)delete r.workflow.asset_ranges[el.dataset.libraryRef];if(el.value)r.workflow.asset_inputs[el.dataset.libraryRef]=el.value;else delete r.workflow.asset_inputs[el.dataset.libraryRef];r.approved=false;}
   if(el.dataset.libraryScalar){if(el.value==='')delete r.workflow.inputs[el.dataset.libraryScalar];else r.workflow.inputs[el.dataset.libraryScalar]=JSON.parse(el.value);r.approved=false;}
   if(el.dataset.librarySize){if(el.value==='')delete r.workflow.inputs[el.dataset.librarySize];else r.workflow.inputs[el.dataset.librarySize]=Number(el.value);r.approved=false;}
   api.markDirty();key='';render();
  });
  listen('#library-editor','click',e=>{
   const b=e.target.closest('[data-library-action]'),r=current();if(!b||!r)return;
   const action=b.dataset.libraryAction;
   if(action==='remove'){if(!confirm('Rimuovere il riferimento dalla produzione? I file originali e le copie nel progetto restano intatti.'))return;api.s.draft.references=api.s.draft.references.filter(v=>v.id!==r.id);for(const sc of api.s.draft.scenes)sc.reference_ids=(sc.reference_ids||[]).filter(id=>id!==r.id);selected='';api.markDirty();key='';api.render();return;}
   if(action==='clear'){r.asset_id=null;r.approved=false;api.markDirty();key='';render();return;}
   return api.run(async()=>{
    const at_ms=Math.round(Number($('#library-frame')?.value||0)*1000),refId=r.id;await api.save();
    const saved=api.s.draft.references.find(r=>r.id===refId);
    if(action==='preview'){await preview(saved);return;}
    if(action==='approve'){if(!review||review.id!==refId||review.asset!==active(saved))throw Error('Apri prima l’anteprima del riferimento attuale.');await api.s.ctx.api(api.endpoint('/references/approve'),'POST',{reference_id:refId,asset_id:review.asset});api.s.ctx.notice('Riferimento approvato per il riuso.');return;}
    const target=action==='generate'?'generate':'analyze';
    if(action==='generate'&&!confirm('Inviare il prompt revisionato al workflow scelto per generare il riferimento?'))return;
    await api.s.ctx.api(api.endpoint('/references/'+target),'POST',{reference_id:refId,operation:action,at_ms});api.s.ctx.notice('Lavoro in coda: avanzamento e risultato restano in questa schermata.');
   });
  });
  listen('#narrative-scenes','change',e=>{const id=e.target.dataset.sceneLibrary;if(!id)return;const scene=api.s.draft.scenes.find(v=>v.id===id),set=new Set(scene.reference_ids||[]);if(e.target.checked)set.add(e.target.value);else set.delete(e.target.value);scene.reference_ids=[...set];scene.visual_approved=false;api.markDirty();});
 }
 function add(kind,asset){
  if(!api.s.project||!api.s.server)throw Error('Scegli un progetto e attendi il caricamento.');
  api.s.draft.references||=[];if(api.s.draft.references.length>=48)throw Error('Massimo 48 riferimenti.');
  const r={id:crypto.randomUUID(),kind,name:asset?.name||'',notes:'',analysis:'',prompt:'',source_asset_id:asset?.id||null,asset_id:null,approved:false,use_for_story:true,workflow:null};api.s.draft.references.push(r);selected=r.id;api.markDirty();key='';render();
 }
 function bindings(r){
  const b=r.workflow;if(!b)return '<p class="muted">Scegli il workflow installato: nessun modello viene imposto.</p>';
  const d=api.s.descriptors.get(api.descriptorKey?api.descriptorKey(b.workflow_id):b.workflow_id);if(!d){api.loadDescriptor(b.workflow_id);return '<p>Lettura ingressi…</p>';}if(d.error)return `<p class="bad">${esc(d.error)}</p>`;
  const target=d.image_target?(()=>{const w=(d.scalar_inputs||[]).find(v=>v.input==='width'),h=(d.scalar_inputs||[]).find(v=>v.input==='height');const field=(v,t)=>(!v||v.editable===false)?'':`<label>${v.input==='width'?'Larghezza':'Altezza'} px<input type="number" min="16" step="16" data-library-size="${esc(v.address)}" value="${b.inputs?.[v.address]??''}" placeholder="${t}"></label>`;return `<p class="muted">Immagine prevista: <strong>${d.image_target.width}×${d.image_target.height}</strong> · ${esc(d.image_target.label)}${d.image_target.capped?' · tetto 4096 applicato':''}. Vuoto = valore calcolato dal video del progetto.</p><div class="narrative-pair">${field(w,d.image_target.width)}${field(h,d.image_target.height)}</div>`;})():'';
  return `${window.NLReferenceRanges.auditMarkup(b,r.prompt,api.s.project,d.reference_inputs||[])}<label>Ingresso del prompt<select data-library-text="1"><option value="">Scegli l’ingresso</option>${(d.text_inputs||[]).filter(v=>v.bindable_text!==false).map(v=>`<option value="${esc(v.address)}" ${b.text_input===v.address?'selected':''}>${esc(v.title||v.class_type)} · ${esc(v.address)}</option>`).join('')}</select></label>${target}${(d.reference_inputs||[]).map(v=>`<label>Reference · ${esc(v.title||v.class_type)} · ${esc(v.address)}<select data-library-ref="${esc(v.address)}" ${v.upload_supported===false?'disabled':''}><option value="">Conserva il workflow</option>${api.s.project.assets.filter(a=>(v.asset_kinds||[v.media_kind]).includes(a.kind)).map(a=>`<option value="${a.id}" ${b.asset_inputs?.[v.address]===a.id?'selected':''}>${esc(a.name)}</option>`).join('')}</select>${v.upload_supported===false?`<span class="bad">${esc(v.reason)}</span>`:''}</label>${window.NLReferenceRanges.markup(b,v,api.s.project,`data-reference-range-id="${r.id}"`)}` ).join('')}<details><summary>Modelli e opzioni originali</summary>${(d.scalar_inputs||[]).filter(v=>v.choices?.length&&v.role!=='reference').map(v=>`<label>${esc(v.title||v.class_type)} · ${esc(v.input)}<select data-library-scalar="${esc(v.address)}"><option value="">Conserva · ${esc(v.value)}</option>${v.choices.map(c=>`<option value="${esc(JSON.stringify(c))}" ${JSON.stringify(b.inputs?.[v.address])===JSON.stringify(c)?'selected':''}>${esc(c)}</option>`).join('')}</select></label>`).join('')}<p class="muted">Seed, dimensioni e impostazioni non modificate restano quelle del workflow. Per impostazioni avanzate usa la sua copia in Generazione.</p></details>`;
 }
 function render(){
  if(!api)return;
  const {s}=api,p=s.project,refs=s.draft?.references||[];
  if(projectId!==p?.id){selected='';review=null;key='';projectId=p?.id;}
  if(!refs.some(r=>r.id===selected))selected=refs[0]?.id||'';
  const r=current(),k=JSON.stringify([refs,selected,p?.assets.map(v=>v.id),[...s.descriptors.keys()]]);
  if(k!==key&&!($('#library-editor').contains(document.activeElement)&&document.activeElement.matches('textarea,input:not([type="checkbox"])'))){
   key=k;const value=$('#library-existing').value;
   $('#library-existing').innerHTML='<option value="">Scegli un media</option>'+(p?.assets||[]).filter(a=>['image','video','audio'].includes(a.kind)).map(a=>`<option value="${a.id}">${esc(a.name)} · ${esc(a.kind)}</option>`).join('');$('#library-existing').value=value;
   $('#library-list').innerHTML=refs.map(v=>`<button class="secondary ${selected===v.id?'selected':''}" data-library-select="${v.id}">${esc(v.name||labels[v.kind])}<small>${esc(labels[v.kind])} · ${v.approved?'approvato':'bozza'}</small></button>`).join('');
   $('#library-editor').innerHTML=r?`<div class="narrative-pair"><label>Tipo<select data-library-field="kind">${Object.entries(labels).map(([k,v])=>`<option value="${k}" ${r.kind===k?'selected':''}>${v}</option>`).join('')}</select></label><label>Nome<input data-library-field="name" maxlength="200" value="${esc(r.name)}"></label></div><label>Media sorgente<select data-library-asset="source_asset_id"><option value="">Nessuno · creazione da descrizione</option>${(p?.assets||[]).filter(a=>r.kind==='voice'?a.kind==='audio':['image','video','audio'].includes(a.kind)).map(a=>`<option value="${a.id}" ${r.source_asset_id===a.id?'selected':''}>${esc(a.name)} · ${esc(a.kind)}</option>`).join('')}</select></label><label>Ruolo nella storia, aspetto, carattere, luogo o indicazioni audio<textarea data-library-field="notes" rows="3" maxlength="8000" placeholder="Cosa vuoi conservare e quale uso fare del materiale?">${esc(r.notes)}</textarea></label><label class="check"><input type="checkbox" data-library-field="use_for_story" ${r.use_for_story?'checked':''}>Usa note, analisi e trascrizione per scrivere la storia</label><div class="actions"><button class="secondary" data-library-action="vision" ${!['image','video'].includes(p.assets.find(a=>a.id===r.source_asset_id)?.kind)?'disabled':''}>Analizza immagine / frame</button><button class="secondary" data-library-action="transcribe" ${!['audio','video'].includes(p.assets.find(a=>a.id===r.source_asset_id)?.kind)?'disabled':''}>Trascrivi parlato italiano</button></div>${p.assets.find(a=>a.id===r.source_asset_id)?.kind==='video'?'<label>Fotogramma da analizzare (secondi)<input id="library-frame" type="number" min="0" step="0.1" value="0"></label>':''}<p class="muted">Visione: modello multimodale esplicitamente scelto in Sistema. Video: un fotogramma, non analisi dell’intero filmato. Audio: trascrizione del parlato, non comprensione musicale.</p><label>Analisi da revisionare / correggere<textarea data-library-field="analysis" rows="3" maxlength="12000">${esc(r.analysis)}</textarea></label>${['character','location','style'].includes(r.kind)?`<details open><summary>${r.kind==='character'?'Character sheet':r.kind==='location'?'Sfondo senza personaggi':'Reference di stile'} · generazione IA</summary><button class="secondary" data-library-action="prompt">Prepara prompt dalle note e dalle analisi</button><label>Prompt da revisionare<textarea data-library-field="prompt" rows="4" maxlength="12000">${esc(r.prompt)}</textarea></label><label>Workflow immagine<select data-library-workflow="1">${api.workflowOptions(r.workflow)}</select></label>${bindings(r)}<button data-library-action="generate" ${r.asset_id?'disabled':''}>Genera riferimento con ComfyUI</button></details>`:''}<label>Risultato / riferimento finale (facoltativo)<select data-library-asset="asset_id"><option value="">Usa il media sorgente</option>${p.assets.filter(a=>r.kind==='voice'?a.kind==='audio':['image','video','audio'].includes(a.kind)).map(a=>`<option value="${a.id}" ${r.asset_id===a.id?'selected':''}>${esc(a.name)} · ${esc(a.kind)}</option>`).join('')}</select></label><div class="actions"><button class="secondary" data-library-action="preview" ${active(r)?'':'disabled'}>Anteprima / ascolta</button><button data-library-action="approve" ${active(r)?'':'disabled'}>Approva riferimento</button><button class="secondary" data-library-action="clear" ${r.asset_id?'':'disabled'}>Sgancia risultato</button><button class="secondary" data-library-action="remove">Rimuovi scheda</button></div><p class="${r.approved?'ok':'muted'}">${r.approved?'Approvato per il riuso nelle scene.':'Da revisionare. Cambiare contenuto annulla l’approvazione.'}</p><p class="muted">L’approvazione non inserisce audio o immagini in timeline. Il riferimento guida la generazione solo se collegato a un ingresso compatibile; una reference vocale non attiva da sola la clonazione.</p>`:'<p class="muted">Importa immagini, video o audio, oppure aggiungi un personaggio o un ambiente. I media originali non saranno modificati.</p>';
  }
  const disabled=!p||!s.server||s.busy||api.running();
  for(const el of document.querySelectorAll('#narrative-library input,#narrative-library textarea,#narrative-library select,#narrative-library button')){if(disabled){if(!el.disabled)el.dataset.libraryBusy='1';el.disabled=true;}else if(el.dataset.libraryBusy){el.disabled=false;delete el.dataset.libraryBusy;}}
 }
 async function preview(r){
  const asset=api.s.project.assets.find(a=>a.id===active(r));if(!asset)throw Error('Media assente.');
  api.stopPreview();api.s.ctx.pausePreview?.();const revision=++api.s.previewRevision,pid=api.s.project.id;
  for(const k of ['image','video','audio'])$('#narrative-preview-'+k).hidden=k!==asset.kind;
  const el=$('#narrative-preview-'+asset.kind);$('#narrative-preview-label').textContent=`${r.name||labels[r.kind]} · ${asset.name}`;$('#narrative-preview-status').textContent='Caricamento…';
  const source=asset.kind==='image'?url(asset.id):await window.MediaPreview.resolve(pid,asset,{onStatus:m=>{if(revision===api.s.previewRevision)$('#narrative-preview-status').textContent=m;}});
  if(revision!==api.s.previewRevision||pid!==api.s.project?.id)return;
  el.src=source;if(asset.kind!=='image')el.load();review={id:r.id,asset:asset.id};$('#narrative-preview-status').textContent='Controlla il riferimento e usa “Approva riferimento” nella scheda. Nessun inserimento automatico in timeline.';
 }
 function sceneMarkup(v){const refs=api?.s.draft?.references||[];return refs.length?`<details class="scene-library"><summary>Personaggi, ambienti e materiali della scena</summary><p class="muted">Seleziona i riferimenti pertinenti. Nei workflow qui sotto assegna i file ai rispettivi ingressi: nessuno scambio automatico di identità.</p>${refs.map(r=>`<label class="check"><input type="checkbox" data-scene-library="${v.id}" value="${r.id}" ${(v.reference_ids||[]).includes(r.id)?'checked':''}>${esc(r.name||labels[r.kind])} · ${r.approved?'approvato':'bozza'}</label>`).join('')}</details>`:'';}
 return{attach,render,sceneMarkup};
})();
