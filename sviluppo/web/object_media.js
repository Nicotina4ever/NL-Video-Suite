'use strict';
window.NLObjectMedia=(()=>{
 const $=s=>document.querySelector(s),esc=v=>String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
 let c,descriptor=null,revision=0;const cache=new Map(),capturedFrames=new Map();
 function configure(context){
  c=context;
  $('#object-workflow').closest('label').firstChild.textContent='Workflow ComfyUI';
  $('#object-description').closest('label').firstChild.textContent='Idea / descrizione e indicazioni creative';
  $('#object-description').placeholder='Descrivi intenzione e stile originali. Queste indicazioni vengono usate anche da Ricava dal parlato.';
  $('#inspector-apply').insertAdjacentHTML('beforebegin','<label>Animazione entrata / uscita<select id="clip-motion"><option value="none">Nessuna</option><option value="fade">Dissolvenza</option><option value="slide_left">Scorrimento da sinistra + dissolvenza</option><option value="slide_up">Scorrimento dal basso + dissolvenza</option></select></label>');
  $('#clip-motion').addEventListener('change',async()=>{try{const clip=c.selected();if(!clip)return;const tl=structuredClone(c.project().timeline),item=tl.clips.find(x=>x.id===clip.id);item.transform={...item.transform,motion:$('#clip-motion').value};await c.api.commit(tl);}catch(e){c.api.notify(e.message,true);}});
  $('#object-title').closest('label').insertAdjacentHTML('beforebegin','<label>Tipo di oggetto<select id="object-media-kind"><option value="image">Immagine / grafica</option><option value="video">Animazione / video</option><option value="audio">Musica / sound design</option></select></label>');
  $('#object-workflow').closest('label').insertAdjacentHTML('afterend','<div id="object-bindings"></div><button type="button" id="object-frame" class="secondary">Cattura frame al cursore</button><p class="muted">Frame dal video originale: selezionalo qui come reference del workflow. Per musica scegli un workflow audio; nessun modello viene installato.</p><audio id="object-review-audio" controls hidden></audio><video id="object-review-video" controls playsinline hidden></video>');
  $('#object-workflow').addEventListener('change',()=>{c.dirty();load(c.object(),true);});
  $('#object-media-kind').addEventListener('change',()=>c.dirty());
  $('#object-bindings').addEventListener('change',()=>c.dirty());
  $('#object-frame').insertAdjacentHTML('afterend','<figure id="object-frame-reference" hidden><img alt="Fotogramma sorgente per il video IA"><figcaption>Fotogramma dal video originale. Collegalo a un ingresso immagine del workflow.</figcaption></figure>');
  $('#object-frame').addEventListener('click',captureFrame);
 }
 async function captureFrame(){
   const button=$('#object-frame');button.disabled=true;
   try{
    await c.save();const p=c.project(),o=c.object(),at=c.time();
    const source=p.timeline.clips.find(x=>x.id===o?.source_clip_id)||c.selected()||[...p.timeline.clips].reverse().find(x=>x.start_ms<=at&&x.start_ms+x.duration_ms>at&&p.assets.find(a=>a.id===x.asset_id)?.kind==='video');
    if(!source||p.assets.find(a=>a.id===source.asset_id)?.kind!=='video'||at<source.start_ms||at>=source.start_ms+source.duration_ms)throw Error('Posiziona il cursore dentro un clip video.');
    c.api.notify('Estrazione del fotogramma originale…');
    const frame=await c.api.api(`/api/projects/${p.id}/assets/${source.asset_id}/frame`,'POST',{at_ms:Math.round(source.trim_in_ms+at-source.start_ms)});
    if(c.project()?.id!==p.id)return;capturedFrames.set(o.id,frame.id);await c.api.reloadProject();await load(c.object(),false);showFrame(c.object());
    const references=[...document.querySelectorAll('[data-object-reference]')].filter(el=>[...el.options].some(op=>op.value===frame.id));
    if(references.length===1){references[0].value=frame.id;c.dirty();}
    c.api.notify('Fotogramma nella libreria. '+(references.length===1?'Reference selezionata: salva l’oggetto.':'Selezionalo nell’ingresso reference del workflow.'));
   }catch(e){c.api.notify(e.message,true);}finally{button.disabled=false;}
    return;
 }
 function showFrame(o){const p=c.project(),id=capturedFrames.get(o.id)||Object.values(o.workflow_binding?.asset_inputs||{}).find(id=>p.assets.some(a=>a.id===id&&a.kind==='image'&&a.source_path?.startsWith('Frame di asset '))),a=p.assets.find(a=>a.id===id);$('#object-frame-reference').hidden=!a;if(a)$('#object-frame-reference img').src=`/api/projects/${p.id}/assets/${a.id}/file`;}
 function render(o){$('#object-media-kind').value=o.media_kind||'image';$('#object-prompt').placeholder=o.media_kind==='audio'?'Atmosfera, tempo/BPM, strumenti, dinamica, sviluppo e finale. Per un sottofondo: niente canto.':o.media_kind==='video'?'Soggetto del frame, azione, movimento camera, continuità e vincoli.':'Soggetto, composizione, materiali, luce e vincoli per una grafica originale.';showFrame(o);load(o,false);}
 async function load(o,changed){
  const id=$('#object-workflow').value,token=++revision;
  if(!id){descriptor=null;$('#object-bindings').innerHTML='<p class="muted">Seleziona un workflow per collegare prompt e reference.</p>';return;}
  const signature=JSON.stringify([o.id,id,o.workflow_binding,c.project().assets.map(a=>a.id)]);
  if(!changed&&descriptor?.workflow_id===id&&$('#object-bindings').dataset.signature===signature)return;
  $('#object-bindings').textContent='Lettura ingressi workflow…';
  try{
   let d=cache.get(id);if(!d){d=await c.api.api(`/api/workflows/${id}/narrative-controls`);cache.set(id,d);}
   if(token!==revision||c.object()?.id!==o.id)return;descriptor=d;
   const b=!changed&&o.workflow_binding?.workflow_id===id?o.workflow_binding:null;
   const assets=c.project().assets;
   $('#object-bindings').dataset.object=o.id;
   $('#object-bindings').dataset.signature=signature;
   $('#object-bindings').innerHTML=`<label>Ingresso per il prompt<select id="object-text-input"><option value="">${($('#object-media-kind').value==='image')?'Automatico (solo immagini compatibili)':'Seleziona il campo testo'}</option>${d.text_inputs.map(v=>`<option value="${esc(v.address)}" ${b?.text_input===v.address?'selected':''}>${esc(v.title||v.class_type)} · ${esc(v.address)}</option>`).join('')}</select></label>`+
    d.reference_inputs.filter(v=>v.upload_supported).map(v=>`<label>Reference · ${esc(v.title||v.class_type)} · ${esc(v.input)}<select data-object-reference="${esc(v.address)}"><option value="">Conserva il workflow</option>${assets.filter(a=>v.asset_kinds.includes(a.kind)&&(!v.requires_audio||a.kind==='audio'||a.media?.has_audio)).map(a=>`<option value="${a.id}" ${b?.asset_inputs?.[v.address]===a.id?'selected':''}>${esc(a.name)}</option>`).join('')}</select></label>`).join('')+
    '<details><summary>Modelli e opzioni installati</summary>'+d.scalar_inputs.filter(v=>v.choices?.length&&v.role!=='reference').map(v=>`<label>${esc(v.title||v.class_type)} · ${esc(v.input)}<select data-object-scalar="${esc(v.address)}"><option value="">Conserva · ${esc(v.value)}</option>${v.choices.map(choice=>`<option value="${esc(JSON.stringify(choice))}" ${JSON.stringify(b?.inputs?.[v.address])===JSON.stringify(choice)?'selected':''}>${esc(choice)}</option>`).join('')}</select></label>`).join('')+'</details>'+ 
    '<details><summary>Parametri: durata, testo, seed…</summary><p class="muted">Modifiche esplicite agli input reali. Cambiare il prompt non cambia automaticamente testi cantati o durata.</p>'+d.scalar_inputs.filter(v=>!v.choices?.length&&v.editable&&v.role!=='reference').map(v=>`<label>${esc(v.title||v.class_type)} · ${esc(v.input)}<textarea rows="2" data-object-literal="${esc(v.address)}" data-type="${typeof v.value}">${esc(typeof v.value==='string'?(b?.inputs?.[v.address]??v.value):JSON.stringify(b?.inputs?.[v.address]??v.value))}</textarea></label>`).join('')+'</details>';
   if(d.text_inputs.some(v=>v.class_type==='TextEncodeAceStepAudio1.5'&&v.input==='tags')){
    $('#object-bindings').insertAdjacentHTML('beforeend','<button type="button" id="object-instrumental" class="secondary">Prepara sottofondo ACE-Step senza canto</button>');
    $('#object-instrumental').onclick=()=>{const tags=d.text_inputs.find(v=>v.class_type==='TextEncodeAceStepAudio1.5'&&v.input==='tags');$('#object-text-input').value=tags.address;$('#object-media-kind').value='audio';for(const v of d.scalar_inputs){const el=[...document.querySelectorAll('[data-object-literal]')].find(e=>e.dataset.objectLiteral===v.address);if(!el)continue;if(v.class_type==='TextEncodeAceStepAudio1.5'&&v.input==='lyrics')el.value='';if((v.class_type==='TextEncodeAceStepAudio1.5'&&v.input==='duration')||(v.class_type==='EmptyAceStep1.5LatentAudio'&&v.input==='seconds'))el.value=String(Number($('#object-duration').value));}c.dirty();c.api.notify('Prompt collegato ai tags, lyrics vuoto e durata impostata a quella dell’oggetto. Modelli e workflow originali invariati. Revisiona e salva.');};
   }
   const captured=capturedFrames.get(o.id),candidates=[...document.querySelectorAll('[data-object-reference]')].filter(el=>[...el.options].some(op=>op.value===captured));
   if(captured&&candidates.length===1&&!candidates[0].value){candidates[0].value=captured;c.dirty();c.api.notify('Fotogramma collegato all’unico ingresso immagine compatibile. Verifica il prompt, salva e genera.');}
  }catch(e){if(token===revision){descriptor=null;$('#object-bindings').textContent=e.message;}}
 }
 function read(o){
  const media_kind=$('#object-media-kind').value,id=$('#object-workflow').value,text_input=$('#object-text-input')?.value;
  const out={media_kind,clear_workflow:!id,clear_workflow_binding:!text_input};
  if(id&&text_input){const old=o.workflow_binding?.workflow_id===id?o.workflow_binding:null,inputs={...old?.inputs},asset_inputs={};
   document.querySelectorAll('[data-object-scalar]').forEach(el=>{if(el.value)inputs[el.dataset.objectScalar]=JSON.parse(el.value);else delete inputs[el.dataset.objectScalar];});
   document.querySelectorAll('[data-object-literal]').forEach(el=>{const address=el.dataset.objectLiteral;if(address===text_input){delete inputs[address];return;}const value=el.dataset.type==='string'?el.value:JSON.parse(el.value),original=descriptor.scalar_inputs.find(v=>v.address===address)?.value;if(JSON.stringify(value)!==JSON.stringify(original))inputs[address]=value;else delete inputs[address];});
   document.querySelectorAll('[data-object-reference]').forEach(el=>{if(el.value)asset_inputs[el.dataset.objectReference]=el.value;});
   out.workflow_binding={workflow_id:id,text_input,inputs,asset_inputs};
  }
  return out;
 }
 function preview(a){for(const kind of ['audio','video']){const el=$('#object-review-'+kind);el.hidden=a?.kind!==kind;if(a?.kind===kind){const url=`/api/projects/${c.project().id}/assets/${a.id}/file`;if(el.getAttribute('src')!==url){el.src=url;el.load();}}else if(el.getAttribute('src')){el.pause();el.removeAttribute('src');el.load();}}}
 return{configure,render,read,preview,captureFrame};
})();
