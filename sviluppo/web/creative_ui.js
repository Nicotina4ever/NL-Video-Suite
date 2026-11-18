'use strict';
// Explicit entry points over the SAME persistent IA objects and review pipeline.
// No canvas template masquerading as AI; no automatic inference on opening.
window.CreativeStudio=(()=>{
 const inspector=$('#object-inspector'),anchor=document.createComment('object-inspector-home');inspector.before(anchor);
 const dialog=document.createElement('dialog');dialog.id='creative-dialog';
 dialog.innerHTML=`<header class="creative-head"><h2 id="creative-title"></h2><button type="button" id="creative-close" class="secondary">Chiudi</button></header><p id="creative-help"></p><p id="creative-notice" role="status"></p><div class="creative-layout"><section><div class="creative-object-nav"><label>Lavori IA di questo progetto<select id="creative-objects"></select></label><button type="button" id="creative-new" class="secondary">Nuovo</button></div><p class="muted">1. Scegli workflow e ingresso del prompt. 2. Scrivi l’idea o ricavala dal parlato. 3. Genera. 4. Revisiona e inserisci. Aprire questo pannello non avvia modelli.</p><div id="creative-editor"></div></section><aside id="creative-audio-library"><h3>Audio già generato o registrato</h3><p>Se hai già un output di ComfyUI, carica quel file. Viene importata una copia: nessuna nuova generazione.</p><label>File audio<input id="creative-audio-file" type="file" accept="audio/*,.wav,.mp3,.flac,.ogg,.m4a,.aac,.opus"></label><button type="button" id="creative-audio-import" class="secondary">Importa audio nel progetto</button><label>Inserimento nella timeline (secondi)<input id="creative-audio-at" type="number" min="0" max="86400" step="0.001"></label><button type="button" id="creative-audio-cursor" class="secondary">Usa il cursore del montaggio</button><label>Volume iniziale (0–1)<input id="creative-audio-volume" type="number" min="0" max="1" step="0.05" value="0.2"></label><div id="creative-audios"></div></aside><aside id="creative-visual-guide"><h3>Direzione originale</h3><p>Descrivi cosa comunicare e come: materiali, composizione, palette, ritmo e spazio per il parlato. Il modello deve interpretare il contenuto, non riprodurre il layout di un altro autore.</p><p>Le indicazioni nel campo descrizione vengono usate quando premi «Ricava dal parlato». I campi partono vuoti: nessun titolo «concetto chiave», icona o scheda preconfezionata entra nel video.</p><p>Per un’animazione generativa scegli tipo Video e un workflow video installato. Per un’immagine, dopo l’approvazione puoi applicare ingresso/uscita nelle proprietà del clip. Nessun modello viene scaricato.</p></aside></div>`;
 document.body.append(dialog);
 let mode='image',projectId=null,assetSignature='',objectSignature='',opening=false,importing=false;
 const isAudio=o=>o?.media_kind==='audio',kindMatches=o=>(mode==='audio')===isAudio(o);
 function notify(text,error=false){if(dialog.open){$('#creative-notice').textContent=text;$('#creative-notice').className=error?'bad':'';}}
 function bindButton(parent,id,text,kind,newDraft){const b=document.createElement('button');b.id=id;b.type='button';b.className='secondary';b.textContent=text;parent.append(b);b.addEventListener('click',()=>open(kind,newDraft).catch(e=>notice(e.message,true)));}
 // Always visible in the editing workspace, without adding width to the timeline toolbar.
 const launch=document.createElement('span');launch.className='creative-launch toolbar-launch';$('#nle-new-object').after(launch);
 bindButton(launch,'nle-ai-graphic','Genera grafica','image',true);bindButton(launch,'nle-ai-audio','Audio e musica','audio',true);
 bindButton(launch,'nle-ai-frame','Anima un frame','video',true);
 const generation=document.createElement('div');generation.className='panel creative-launch';$('#tab-workflows').prepend(generation);
 bindButton(generation,'generation-ai-graphic','Grafiche IA · genera e inserisci','image',false);bindButton(generation,'generation-ai-audio','Musica e audio · genera / importa / inserisci','audio',false);
 bindButton(generation,'generation-ai-frame','Frame → video IA','video',true);
 async function open(kind,newDraft=false,options={}){
  if(opening)return;opening=true;
  try{const p=project();mode=kind;projectId=p.id;assetSignature='';objectSignature='';NLE.pause();
   const existing=[...(p.graphic_objects||[])].reverse().find(kindMatches);
   if(newDraft||!existing){if(!await NLE.newObject(kind))return;}else{if(NLE.getObject()?.id!==existing.id)NLE.selectObject(existing.id);if(NLE.getObject()?.id!==existing.id)return;}
   if(state.project?.id!==projectId)return;
   $('#creative-title').textContent=kind==='audio'?'Musica e audio IA':kind==='video'?'Video IA da un fotogramma':'Grafica e animazione IA';
   $('#creative-help').textContent=kind==='audio'?'Genera con un workflow audio installato, ascolta il risultato e approva l’inserimento. Oppure importa qui un audio già prodotto in ComfyUI.':'Genera immagini o video reali con ComfyUI, partendo dal discorso e dalle tue indicazioni. Nessun layout di un altro autore è preimpostato.';
   if(kind==='video')$('#creative-help').textContent='Il frame al cursore viene estratto dal video originale. Scegli un workflow image-to-video e il campo prompt; se esiste un solo ingresso immagine compatibile il frame viene collegato, altrimenti scegli tu il loader. Descrivi movimento e continuità, genera e revisiona prima dell’inserimento.';
   $('#creative-audio-library').hidden=kind!=='audio';$('#creative-visual-guide').hidden=kind==='audio';
   $('#creative-audio-at').value=(NLE.getTime()/1000).toFixed(3);$('#creative-notice').textContent='';dialog.dataset.libraryOnly=options.libraryOnly&&kind==='image'?'true':'false';if(dialog.dataset.libraryOnly==='true')$('#creative-help').textContent='Produci una risorsa per la composizione Motion. Il risultato viene importato nella libreria: chiudi questo pannello e collegalo nella proposta. Nessun overlay duplicato viene inserito nella timeline.';
   $('#creative-editor').append(inspector);if(!dialog.open)dialog.showModal();refresh();if(kind==='video')await NLObjectMedia.captureFrame();
  }finally{opening=false;}
 }
 function close(){dialog.close();$('#object-approve').hidden=false;dialog.dataset.libraryOnly='false';}
 $('#creative-close').onclick=close;
 dialog.addEventListener('close',()=>{dialog.querySelectorAll('audio,video').forEach(el=>el.pause());anchor.after(inspector);NLE.refreshControls();});
 $('#creative-new').onclick=async()=>{try{await NLE.newObject(mode);refresh();}catch(e){notify(e.message,true);}};
 $('#creative-objects').onchange=()=>{NLE.selectObject($('#creative-objects').value);$('#creative-objects').value=NLE.getObject()?.id||'';};
 $('#creative-audio-cursor').onclick=()=>{$('#creative-audio-at').value=(NLE.getTime()/1000).toFixed(3);};
 function refresh(){
  if(!dialog.open)return;if(state.project?.id!==projectId){close();return;}
  $('#object-approve').hidden=dialog.dataset.libraryOnly==='true';const p=project(),objects=(p.graphic_objects||[]).filter(kindMatches),sig=JSON.stringify(objects.map(o=>[o.id,o.title,o.status]));
  if(sig!==objectSignature){objectSignature=sig;$('#creative-objects').innerHTML=objects.map(o=>`<option value="${o.id}">${escapeHTML(o.title)} · ${escapeHTML(({draft:'bozza',described:'prompt pronto',describing:'analisi',generating:'generazione',review:'da approvare',approved:'inserito',failed:'errore'})[o.status]||o.status)}</option>`).join('');}
  $('#creative-objects').value=NLE.getObject()?.id||'';
  if(mode!=='audio')return;
  const audios=p.assets.filter(a=>a.kind==='audio'),next=JSON.stringify(audios.map(a=>[a.id,a.name]));if(next===assetSignature)return;assetSignature=next;
  const list=$('#creative-audios');
  // Preserve loaded players and buttons while an import finishes. Replacing
  // the whole list interrupts listening and can swallow an in-progress click.
  if(!audios.length){list.innerHTML='<p class="muted">Nessun audio importato. Genera a sinistra oppure carica un file qui.</p>';return;}
  list.querySelectorAll(':scope > p').forEach(el=>el.remove());
  list.querySelectorAll('[data-audio-row]').forEach(el=>{if(!audios.some(a=>a.id===el.dataset.audioRow))el.remove();});
  for(const a of audios){let row=list.querySelector(`[data-audio-row="${a.id}"]`);if(!row){row=document.createElement('article');row.className='creative-audio';row.dataset.audioRow=a.id;row.innerHTML=`<strong></strong><audio controls preload="metadata" src="/api/projects/${p.id}/assets/${a.id}/file"></audio><button type="button" data-creative-audio="${a.id}">Inserisci questo audio in timeline</button>`;list.append(row);}row.querySelector('strong').textContent=a.name;}
 }
 $('#creative-audio-import').onclick=async()=>{
  if(importing)return;const file=$('#creative-audio-file').files[0];if(!file){notify('Scegli il file audio prodotto dal tuo workflow.',true);return;}
  if(!/\.(wav|mp3|flac|ogg|m4a|aac|opus)$/i.test(file.name)){notify('Scegli un file audio WAV, MP3, FLAC, OGG, M4A, AAC o OPUS.',true);return;}
  importing=true;$('#creative-audio-import').disabled=true;const pid=projectId;
  try{const form=new FormData();form.append('file',file);notify('Importazione di una copia dell’audio…');const r=await fetch(`/api/projects/${pid}/assets/upload`,{method:'POST',body:form});const job=await r.json();if(!r.ok)throw Error(job.error||'Importazione non riuscita');
   let done;const deadline=Date.now()+120000;while(Date.now()<deadline){done=await api('/api/jobs/'+job.id);if(['succeeded','failed'].includes(done.status))break;await new Promise(r=>setTimeout(r,500));}
   if(done?.status!=='succeeded')throw Error(done?.message||'Importazione ancora in corso: controlla Lavori.');
   if(state.project?.id!==pid)return;await loadProject(pid);notify('Audio importato. Ascoltalo qui, poi scegli «Inserisci questo audio in timeline».');$('#creative-audio-file').value='';
  }catch(e){notify(e.message,true);}finally{importing=false;$('#creative-audio-import').disabled=false;}
 };
 $('#creative-audios').onclick=async e=>{
  const button=e.target.closest('[data-creative-audio]');if(!button||button.disabled)return;button.disabled=true;
  try{const p=project();if(p.id!==projectId)throw Error('Il progetto selezionato è cambiato.');const a=p.assets.find(a=>a.id===button.dataset.creativeAudio&&a.kind==='audio');if(!a)throw Error('Audio non disponibile.');
   const at=Math.round(Number($('#creative-audio-at').value)*1000),volume=Number($('#creative-audio-volume').value);if(!Number.isFinite(at)||at<0||!Number.isFinite(volume)||volume<0||volume>1)throw Error('Controlla tempo e volume.');
   const tl=copy(p.timeline);let track=tl.tracks.find(t=>t.kind==='audio'&&!t.locked);if(!track){if(tl.tracks.some(t=>t.kind==='audio'))throw Error('Sblocca una traccia audio prima di inserire.');track={id:crypto.randomUUID(),name:'Audio',kind:'audio',visible:true,locked:false};tl.tracks.push(track);}
   const duration=a.media?.duration_ms;if(!duration)throw Error('Durata audio non rilevata: verifica il file importato.');
   tl.clips.push({id:crypto.randomUUID(),asset_id:a.id,track_id:track.id,start_ms:at,trim_in_ms:0,duration_ms:duration,label:a.name,volume,opacity:1});await commitTimeline(tl);notify('Audio inserito nella traccia audio. Il file originale è rimasto intatto.');
  }catch(error){notify(error.message,true);}finally{button.disabled=false;}
 };
 return{open,refresh,notify};
})();
