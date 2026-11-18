'use strict';
// One widget/contract for scene workflows and reference-library workflows.
window.NLReferenceRanges=(()=>{
 const esc=v=>String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
 function markup(binding,control,project,attrs=''){
  if(control.media_kind!=='audio'||!control.upload_supported||!binding.asset_inputs?.[control.address])return'';
  const address=esc(control.address),range=binding.asset_ranges?.[control.address];
  return`<fieldset class="reference-audio-range"><legend>Reference audio · ${address}</legend><label>Materiale da inviare<select data-ref-mode="${address}" ${attrs}><option value="whole" ${range?'':'selected'}>File intero</option><option value="range" ${range?'selected':''}>Intervallo nel file originale</option></select></label>${range?`<div class="narrative-pair"><label>Inizio (s)<input type="number" min="0" step=".001" data-ref-start="${address}" ${attrs} value="${range.start_ms/1000}"></label><label>Fine (s)<input type="number" min="0" step=".001" data-ref-end="${address}" ${attrs} value="${range.end_ms/1000}"></label></div>`:''}<p class="muted">${range?'Si invia soltanto la finestra scelta, con cache locale. Per LoadAudioUI, inizio/fine/durata locali vengono riallineati alla finestra estratta nella copia inviata (modifiche registrate), evitando un secondo ritaglio. ':''}Primo flusso audio, frequenza e canali della sorgente, senza normalizzazione. È compatibilità tecnica del loader: non prova clonazione vocale, identità o comprensione semantica.</p></fieldset>`;
 }
 function change(binding,el,project){
  const address=el.dataset.refMode||el.dataset.refStart||el.dataset.refEnd;if(!address)return false;
  if(!binding)throw Error('Workflow di reference non disponibile.');binding.asset_ranges||={};
  if(el.dataset.refMode&&el.value==='whole'){delete binding.asset_ranges[address];return true;}
  const asset=project.assets.find(a=>a.id===binding.asset_inputs?.[address]);if(!asset?.media?.has_audio||!asset.media.duration_ms)throw Error('La reference richiede audio e durata verificati.');
  if(el.dataset.refMode){binding.asset_ranges[address]={start_ms:0,end_ms:asset.media.duration_ms};return true;}
  const box=el.closest('fieldset'),fields=[...box.querySelectorAll('input')],start=Number(box.querySelector('[data-ref-start]').value)*1000,end=Number(box.querySelector('[data-ref-end]').value)*1000;for(const input of fields)input.setCustomValidity('');
  if(!Number.isFinite(start)||!Number.isFinite(end)||start<0||end<=start||end>asset.media.duration_ms){const message='Finestra della reference non valida: scegli inizio e fine entro il file.';el.setCustomValidity(message);throw Error(message);}
  binding.asset_ranges[address]={start_ms:Math.round(start),end_ms:Math.round(end)};return true;
 }
 function validateAll(draft,project){for(const el of document.querySelectorAll('#narrative-scenes [data-ref-start],#library-editor [data-ref-start]')){
  const binding=el.dataset.sceneId?draft.scenes.find(s=>s.id===el.dataset.sceneId)?.[el.dataset.kind+'_workflow']:draft.references?.find(r=>r.id===el.dataset.referenceRangeId)?.workflow;
  change(binding,el,project);
 }}
 function audit(binding,text,project,controls=[]){
  const basename=v=>String(v??'').replace(/\\/g,'/').split('/').pop(),normalize=v=>basename(v).normalize('NFC').toLocaleLowerCase();
  const connected=controls.map(c=>{const id=binding?.asset_inputs?.[c.address],asset=(project?.assets||[]).find(a=>a.id===id);return{address:c.address,asset_id:id||null,name:asset?.name||String(binding?.inputs?.[c.address]??c.value??''),source:asset?basename(asset.source_path):null,file:asset?.file_name||null,status:asset?'project_asset':id?'missing_project_asset':'workflow_literal_unverified'};});
  const names=new Set(),media=/\.(png|jpe?g|webp|gif|bmp|svg|wav|mp3|flac|ogg|m4a|aac|mp4|mov|mkv|webm)$/i;
  for(const m of String(text||'').matchAll(/<([^<>\n]{1,200})>/g)){const v=m[1].trim();if(media.test(v)||/^(audio|image|video|foto|immagine|reference|ref)[ _-]?\d+$/i.test(v))names.add(v);}
  for(const m of String(text||'').matchAll(/["']([^"'\n]{1,200}\.(?:png|jpe?g|webp|svg|wav|mp3|flac|m4a|mp4|mov|mkv|webm))["']/gi))names.add(m[1]);
  for(const m of String(text||'').matchAll(/\b[\w-]+\.(?:png|jpe?g|webp|svg|wav|mp3|flac|m4a|mp4|mov|mkv|webm)\b/gi)){if(![...names].some(n=>n.endsWith(m[0])))names.add(m[0]);}
  const unresolved=[...names].filter(n=>!connected.some(c=>[c.name,c.source,c.file].some(v=>v&&normalize(v)===normalize(n))));
  return{connected,mentions:[...names],unresolved};
 }
 function auditMarkup(binding,text,project,controls=[]){
  const result=audit(binding,text,project,controls);if(!result.connected.length&&!result.mentions.length)return'';
  return`<details class="reference-name-audit" ${result.unresolved.length?'open':''}><summary>Reference nel prompt e collegamenti${result.unresolved.length?' · da verificare':''}</summary>${result.unresolved.length?`<p class="bad" role="status">Nomi non risolti dai collegamenti: ${result.unresolved.map(esc).join(', ')}. Possono essere alias: chiariscili o correggi prompt e collegamenti. Nessuna reference viene sostituita.</p>`:''}<ul>${result.connected.map(c=>`<li>${esc(c.address)} → ${esc(c.name)}${c.source?` · ${esc(c.source)}`:''} · ${c.status==='project_asset'?'asset del progetto':c.status==='missing_project_asset'?'asset non disponibile':'valore del workflow, file remoto non verificato'}</li>`).join('')}</ul><p class="muted">Confronto dei nomi espliciti, non riconoscimento del contenuto o dell’identità. Un nome corrispondente non prova continuità, lip-sync o supporto semantico.</p></details>`;
 }
 return{markup,change,validateAll,audit,auditMarkup};
})();
