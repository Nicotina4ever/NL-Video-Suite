'use strict';
// Task-focused presentation of the existing editors. No copied project state or pipeline.
window.EditingWorkspace=(()=>{
 const $=q=>document.querySelector(q);
 function configure(h){
  const studio=$('#tab-studio'),bar=studio.querySelector('.workspace-controls'),more=$('#editor-extra-tools');
  studio.classList.add('human-editor');
  if(studio.classList.contains('active'))$('#work-context').hidden=true;
  new ResizeObserver(()=>window.dispatchEvent(new Event('resize'))).observe($('#work-context'));
  for(const node of [...bar.children])if(node!==more)more.lastElementChild.append(node);
  more.querySelector('summary').textContent='Opzioni';
  for(const el of [...more.querySelectorAll('button')])if(el.textContent==='Foto / video / musica'||el.dataset.workspace==='edit')el.remove();
  for(const id of ['production-package-open','reload'])if($('#'+id))more.lastElementChild.append($('#'+id));
  const button=(id,text,fn)=>{const b=document.createElement('button');b.type='button';b.id=id;b.className='secondary';b.textContent=text;b.onclick=fn;return b;};
  const back=button('editor-back','← Torna al montaggio',()=>task(''));
  const assemble=button('editor-assemble','Crea montaggio',()=>task('assembly'));
  const write=$('#album-resume');write.textContent='Scrivi dalle foto';write.onclick=()=>task('writing');
  // It must be discoverable before any analysis exists, including an empty album.
  const writingObserver=new MutationObserver(()=>{const expected=Boolean(studio.dataset.editorTask);if(write.hidden!==expected)write.hidden=expected;});writingObserver.observe(write,{attributes:true,attributeFilter:['hidden']});write.hidden=false;
  const add=document.createElement('details');add.id='editor-add';add.innerHTML='<summary>+ Aggiungi</summary><div class="actions"></div>';
  for(const id of ['work-title','work-graphic','nle-ai-graphic','nle-ai-audio','nle-ai-frame','nle-new-object'])if($('#'+id))add.lastElementChild.append($('#'+id));
  add.querySelector('#work-title').textContent='Grafica o animazione…';add.querySelector('#work-graphic').textContent='Proponi grafiche dal parlato';
  add.querySelector('#work-title').addEventListener('click',()=>window.MotionIdea.focus());
  add.addEventListener('click',e=>{if(e.target.closest('button'))add.open=false;});
  const adjust=button('editor-adjust','Regola clip',()=>{studio.classList.toggle('show-clip-controls');adjust.setAttribute('aria-expanded',studio.classList.contains('show-clip-controls'));window.dispatchEvent(new Event('resize'));});adjust.setAttribute('aria-expanded','false');
  const versions=$('#versions-open');if(versions)bar.append(versions);
  bar.prepend(back,assemble,write,add,adjust);bar.append(more);back.hidden=true;
  const inspector=studio.querySelector('.inspector');inspector.prepend(button('editor-close-properties','← Chiudi proprietà',()=>adjust.click()));
  // Basic clip edits first; coordinates, source trim and routing remain available.
  const basic=$('#clip-inspector .inspector-grid'),advanced=document.createElement('details');advanced.id='clip-advanced';advanced.innerHTML='<summary>Posizione, ritaglio e parametri avanzati</summary><div class="inspector-grid"></div>';
  for(const el of [...basic.children])if(!['label','duration_ms','volume'].includes(el.querySelector('[data-inspect]')?.dataset.inspect))advanced.lastElementChild.append(el);
  basic.after(advanced);
  // One import affordance in the shared library instead of two permanently open forms.
  const library=$('.library'),imports=document.createElement('details');imports.id='editor-import';imports.innerHTML='<summary>+ Importa materiali</summary>';
  library.querySelector('.panelhead').after(imports);imports.append($('#upload-form'),$('#import-form'));$('#upload-form button').textContent='Importa file';$('#import-form').querySelector('label').firstChild.textContent='Oppure incolla un percorso';
  const upload=$('#upload-file');upload.multiple=true;
  let writingImport=false;
  const importSequence=button('assembly-import','+ Importa materiali',()=>{writingImport=true;upload.accept='image/*,video/*,audio/*';upload.click();});$('#montage-add-all').before(importSequence);
  const addPhotos=button('writing-import','+ Aggiungi foto',()=>{writingImport=true;upload.accept='image/*';upload.click();});$('#cue-coverage').before(addPhotos);
  upload.addEventListener('change',()=>{if(writingImport&&upload.files.length){writingImport=false;$('#upload-form').requestSubmit();}});
  imports.addEventListener('toggle',()=>{if(imports.open){writingImport=false;upload.accept='';}});
  $('#upload-form').addEventListener('submit',async e=>{e.preventDefault();e.stopImmediatePropagation();const files=[...upload.files],p=h.project();if(!p)return h.notice('Crea o scegli un progetto.',true);const submit=$('#upload-form button');submit.disabled=true;try{for(let i=0;i<files.length;i++){h.notice(`Importazione ${i+1}/${files.length} · ${files[i].name}`);const body=new FormData();body.append('file',files[i]);const r=await fetch(`/api/projects/${p.id}/assets/upload`,{method:'POST',body}),j=await r.json();if(!r.ok)throw Error(j.error||'Importazione non riuscita');await h.wait(j);}if($('#album-cue-studio').open)await window.NLAlbumScript.refreshMaterials();upload.value='';imports.open=false;h.notice(studio.dataset.editorTask==='writing'?`${files.length} foto aggiunte al copione.`:studio.dataset.editorTask==='assembly'?'Materiali pronti: scegli quelli da usare nella sequenza.':`${files.length} materiali importati. Aggiungili alla timeline dalla libreria.`);}catch(e){h.notice(e.message,true);}finally{submit.disabled=false;}},true);
  const sourceOptions=document.createElement('details');sourceOptions.id='work-processing-options';sourceOptions.innerHTML='<summary>Dispositivo di elaborazione</summary>';$('#work-device').closest('label').before(sourceOptions);sourceOptions.append($('#work-device').closest('label'));
  // Keep selection state in the native NLE; the event only updates presentation.
  function selection(e){const id=e?.detail?.clip_id;adjust.disabled=!id;adjust.textContent=id?'Regola clip':'Seleziona un clip per regolarlo';}
  document.addEventListener('nl:clip-selection',selection);selection();
  const extra=document.createElement('details');extra.id='timeline-extra';extra.innerHTML='<summary>Altri comandi</summary><div class="actions"></div>';
  $('.nle-toolbar').append(extra);
  for(const id of ['nle-ripple','nle-duplicate','nle-track-kind','nle-add-track','nle-snap']){const el=$('#'+id);if(el)extra.lastElementChild.append(el.matches('input,select')?el.closest('label'):el);}
  $('#nle-snap').closest('label').lastChild.textContent='Aggancia i tagli';$('.nle-toolbar h2').textContent='Sequenza';$('#nle-remove').textContent='Elimina';
  const divider=$('[data-workspace-resize="height"]');divider.textContent='⋯ Trascina per ridimensionare la timeline ⋯';divider.setAttribute('aria-label','Altezza timeline: trascina verso l’alto per estenderla');divider.title='Trascina verso l’alto per vedere più tracce · frecce per regolare · doppio clic per ripristinare';
  // One brief and sequence first; detailed timing and model instructions are optional.
  const montageOptions=document.createElement('details');montageOptions.id='montage-options';montageOptions.innerHTML='<summary>Ritmo, transizioni e impostazioni avanzate</summary>';
  const side=$('.montage-layout>aside');side.append(montageOptions);
  for(const id of ['montage-target','montage-bpm','montage-beat-offset','montage-pace','montage-transition','montage-title-design','montage-volume']){const el=$('#'+id);if(el)montageOptions.append(el.closest('label')||el);}
  for(const p of [...side.querySelectorAll(':scope>p')])montageOptions.append(p);
  $('#montage-brief').closest('label').firstChild.textContent='Che montaggio vuoi creare?';
  const individual=document.createElement('details');individual.innerHTML='<summary>Scegli materiali e ordinamento</summary>';$('#montage-library').before(individual);individual.append($('#montage-library'));const sorting=$('#montage-sort-name').parentElement,dateNote=sorting.nextElementSibling;individual.append(sorting);if(dateNote?.matches('p'))individual.append(dateNote);
  const importActions=$('#montage-files').closest('.actions');imports.append($('#montage-files').closest('label'),$('#montage-import'));$('#montage-add-all').textContent='Usa foto e video del progetto';importActions.prepend($('#montage-add-all'));
  const planner=$('#montage-planner'),directorOptions=document.createElement('details');directorOptions.id='planner-options';directorOptions.innerHTML='<summary>Indicazioni aggiuntive, reference e modello</summary>';planner.querySelector('.planner-controls').before(directorOptions);directorOptions.append(planner.querySelector('.planner-controls'));
  // A previous, explicitly written AI instruction is retained and remains editable here.
  planner.querySelector('header h3').textContent='Vuoi una proposta di montaggio dall’IA?';
  const assemblyActions=$('#montage-save').parentElement,proposal=$('#montage-proposal');assemblyActions.classList.add('montage-sequence-actions');
  $('#montage-brief').closest('label').after(assemblyActions);assemblyActions.after($('#montage-status'),$('#montage-error'),proposal);
  $('#montage-save').textContent='Salva scelte';$('#montage-preview').textContent='Controlla sequenza';$('#montage-apply').textContent='Inserisci nella timeline';
  for(const b of [...assemblyActions.querySelectorAll('button')])if(b.textContent==='Rifinisci nella timeline')b.remove();
  const director=document.createElement('details');director.id='montage-director';director.innerHTML='<summary>Regia IA · opzioni e proposte</summary>';planner.before(director);director.append(planner);
  const suggest=$('#planner-propose');suggest.textContent='Suggerisci ordine e stile con IA';assemblyActions.append(suggest);suggest.addEventListener('click',()=>{director.open=true;});
  proposal.after($('#planner-status'));
  // The task view exposes one existing editor at a time, without resetting drafts.
  function task(name){
   if(name){window.NLE.pause();$('#preview').pause();}if(name!=='assembly')$('#montage-source-video')?.pause();
   studio.dataset.editorTask=name;back.hidden=!name;assemble.hidden=Boolean(name);write.hidden=Boolean(name);add.hidden=Boolean(name);adjust.hidden=Boolean(name);if(versions)versions.hidden=Boolean(name);more.hidden=Boolean(name);
   const montage=$('#montage-studio');montage.open=Boolean(name);
   if(name==='writing'){$('#album-studio').open=true;window.NLAlbumScript.open();}
   else if(name==='assembly'){$('#album-studio').open=false;$('#montage-studio').scrollIntoView({block:'start'});}
   window.dispatchEvent(new Event('resize'));requestAnimationFrame(()=>window.scrollTo(0,0));
  }
  document.addEventListener('nl:montage-applied',()=>task(''));
  // Existing entry points (including old sessions) join the same focused view.
  $('#montage-studio').addEventListener('toggle',()=>{if(!$('#montage-studio').open&&studio.dataset.editorTask)task('');});
  requestAnimationFrame(()=>window.dispatchEvent(new Event('resize')));
  return{task};
 }
 return{configure};
})();
