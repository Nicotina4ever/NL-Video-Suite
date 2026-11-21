'use strict';
window.SocialThumbnail=(()=>{
 const $=q=>document.querySelector(q),defaults={at_ms:0,width:1280,height:720,title:'',font:'Arial',color:'#FFFFFF',font_size:.07,x:.5,y:.8,title_width:.85,backing:true,include_motion:false,include_captions:false,source_asset_id:null};let host,projectId,previewKey='';
 function configure(h){host=h;const box=document.createElement('section');box.className='panel';box.id='social-thumbnail';box.innerHTML=`<div class="panelhead"><div><p class="eyebrow">FRAME / TYPOGRAPHY · NO INFERENCE</p><h3>Copertina</h3></div></div><p class="muted">Scegli un frame del montaggio e componi il titolo. Il PNG usa il compositor, non una cattura del monitor. Scegli se includere motion e CC; partono esclusi per una copertina pulita. I media e gli overlay già montati sulle tracce video restano visibili. Titolo e stile restano salvati nel progetto, il PNG è la consegna raster.</p><div class="lf-config"><label>Frame timeline (s)<input data-thumb="at_ms" type="number" min="0" step=".001"></label><button id="social-thumb-current" class="secondary">Usa il playhead</button><label>Larghezza (px)<input data-thumb="width" type="number" min="64" max="4096" step="2"></label><label>Altezza (px)<input data-thumb="height" type="number" min="64" max="4096" step="2"></label></div><div class="actions"><label><input data-thumb="include_motion" type="checkbox"> Includi motion approvate</label><label><input data-thumb="include_captions" type="checkbox"> Includi sottotitoli attivi</label></div><label>Titolo editabile (vuoto = solo frame)<textarea data-thumb="title" maxlength="250" rows="2"></textarea></label><div class="lf-config"><label>Font<select data-thumb="font"></select></label><label>Colore<input data-thumb="color" type="color"></label><label>Testo (%H)<input data-thumb="font_size" type="number" min="2" max="18" step=".1"></label><label>Centro X (%)<input data-thumb="x" type="number" min="5" max="95"></label><label>Centro Y (%)<input data-thumb="y" type="number" min="5" max="95"></label><label>Larghezza titolo (%)<input data-thumb="title_width" type="number" min="10" max="95"></label><label><input data-thumb="backing" type="checkbox"> Fondo scuro leggibile</label></div><div class="actions"><button id="social-thumb-render">Crea / aggiorna copertina</button><a id="social-thumb-download" hidden download="thumbnail.png">Scarica PNG</a><label><input id="social-thumb-small" type="checkbox" checked> Controllo leggibilità a320px</label></div><p id="social-thumb-status" class="muted" role="status"></p><img id="social-thumb-image" alt="Miniatura renderizzata dal compositor nativo" hidden style="display:block;width:320px;max-width:100%;height:auto;border-radius:8px"><p class="muted">Verifica contrasto, titolo non troncato e soggetto libero anche nel formato piccolo. Nessun punteggio automatico può certificare il gusto o l’efficacia di una miniatura.</p>`;const body=document.createElement('div');body.className='thumb-body';const controls=document.createElement('div');controls.className='thumb-controls';const review=document.createElement('aside');review.className='thumb-review';while(box.children.length>1)controls.append(box.children[1]);const heading=document.createElement('h4');heading.textContent='Anteprima · consegna PNG';review.append(heading);review.append(controls.querySelector('#social-thumb-image'),controls.querySelector('#social-thumb-status'),controls.lastElementChild);review.querySelector('img').style.removeProperty('display');body.append(controls,review);box.append(body);$('#tab-social>.lf-layout').after(box);attachAI(box);
  const designOptions=document.createElement('details');designOptions.innerHTML='<summary>Stile, dimensioni e livelli · facoltativi</summary>';controls.insertBefore(designOptions,controls.querySelector('#social-thumb-render').closest('.actions'));for(const key of ['width','height'])designOptions.append(controls.querySelector(`[data-thumb="${key}"]`).closest('label'));designOptions.append(controls.querySelector('[data-thumb="font"]').closest('.lf-config'),controls.querySelector('[data-thumb="include_motion"]').closest('.actions'));controls.querySelector('.muted').hidden=true;
  controls.prepend(Object.assign(document.createElement('label'),{innerHTML:'Sorgente della copertina<select data-thumb="source_asset_id"></select>'}));review.insertAdjacentHTML('afterbegin','<video id="thumb-source-video" controls playsinline preload="metadata" hidden></video><img id="thumb-source-image" alt="Sorgente della copertina" hidden><p id="thumb-source-note" class="muted"></p>');
  box.addEventListener('change',e=>{const key=e.target.dataset.thumb;if(!key)return;const d=host.document();if(!d)return;d.thumbnail={...defaults,...d.thumbnail};d.thumbnail[key]=key==='source_asset_id'?(e.target.value||null):e.target.type==='checkbox'?e.target.checked:e.target.type==='number'?Number(e.target.value)*(key==='at_ms'?1000:['font_size','x','y','title_width'].includes(key)?.01:1):e.target.value;$('#social-thumb-status').textContent='Design modificato: anteprima precedente. Prepara nuovamente il PNG.';host.changed();if(key==='source_asset_id'){d.thumbnail.at_ms=0;refresh();}else if(key==='at_ms')refresh();});
  $('#social-thumb-current').onclick=()=>{const d=host.document();if(!d)return;d.thumbnail={...defaults,...d.thumbnail,at_ms:Math.round(d.thumbnail?.source_asset_id?($('#thumb-source-video').currentTime||0)*1000:window.NLE.getTime())};host.changed();refresh();};
  $('#social-thumb-small').onchange=e=>$('#social-thumb-image').style.width=e.target.checked?'320px':'100%';
  $('#social-thumb-render').onclick=()=>host.run(async()=>{await host.save();const p=host.project();$('#social-thumb-status').textContent='Preparazione del frame…';const j=await host.ctx.api('/api/v1/projects/'+p.id+'/social/thumbnail','POST',{expected_updated_at:p.updated_at});await host.ctx.refreshJobs();let done;for(let i=0;i<240;i++){done=await host.ctx.api('/api/jobs/'+j.id);$('#social-thumb-status').textContent=done.message;if(['failed','succeeded'].includes(done.status))break;await new Promise(r=>setTimeout(r,500));}if(done.status!=='succeeded')throw Error(done.message||'Miniatura ancora in coda.');if(host.project()?.id!==p.id)return;show(done);host.ctx.notice('Miniatura pronta: controllala anche a dimensione telefono.');}).catch(host.error);
 }
 function attachAI(box){
  const details=document.createElement('details');details.id='thumb-ai';details.open=true;
  details.innerHTML=`<summary>Copertina AI dal frame</summary>
   <p class="muted">Scegli un workflow con input testo e reference immagine: riceve il frame della sorgente scelta e la descrizione. Nulla parte senza il pulsante.</p>
   <label>Workflow<select id="thumb-ai-workflow"><option value="">Carica catalogo…</option></select></label>
   <label>Descrizione<textarea id="thumb-ai-prompt" rows="3" maxlength="4000" placeholder="Es. copertina: volto sorpreso a sinistra, titolo grande a destra, colori caldi e leggibili"></textarea></label>
   <div class="actions"><button id="thumb-ai-context" class="secondary">Prendi il parlato qui</button><button id="thumb-ai-generate" disabled>Genera copertina</button></div>
   <p id="thumb-ai-info" class="muted"></p><p id="thumb-ai-status" class="muted" role="status"></p>`;
  box.append(details);
  const $i=id=>details.querySelector('#'+id);
  let workflows=[],controls=null,workflow=null,busy=false;
  host.ctx.api('/api/workflows').then(list=>{workflows=list;$i('thumb-ai-workflow').innerHTML='<option value="">Scegli un workflow…</option>'+list.map(w=>`<option value="${w.id}">${w.name.replace(/[<>&"]/g,'')} - ${w.node_count} nodi</option>`).join('');}).catch(e=>{$i('thumb-ai-info').textContent=e.message;});
  async function loadControls(){
    const id=$i('thumb-ai-workflow').value;workflow=workflows.find(w=>w.id===id)||null;controls=null;
    if(!workflow){$i('thumb-ai-info').textContent='';$i('thumb-ai-generate').disabled=true;return;}
    $i('thumb-ai-info').textContent='Lettura controlli del workflow…';
    try{
      const p=host.project();
      controls=await host.ctx.api(`/api/workflows/${id}/narrative-controls?project_id=${p.id}`);
      const score=c=>{const n=String(c.input||'').toLowerCase(),t=String(c.title||'').toLowerCase(),r=String(c.role||'').toLowerCase();let s=0;if(n==='prompt')s+=14;if(n==='text')s+=12;if(n==='string')s+=8;if(n.includes('positive'))s+=10;if(t.includes('prompt'))s+=8;if(t.includes('text'))s+=5;if(r.includes('prompt'))s+=12;if(['delimiter','separator','prefix','suffix','negative'].includes(n))s-=25;return s;};
      const text=[...controls.text_inputs].filter(c=>c.bindable_text).sort((a,b)=>score(b)-score(a))[0]||controls.text_inputs[0];
      const ref=controls.reference_inputs.find(c=>Array.isArray(c.asset_kinds)&&c.asset_kinds.some(k=>['video','image'].includes(k)));
      controls._text=text;controls._ref=ref;
      $i('thumb-ai-info').textContent=(text?`Testo: ${text.address}`:'Nessun input testo')+' - '+(ref?`Frame: ${ref.address}`:'Nessun input immagine (solo testo)');
      $i('thumb-ai-generate').disabled=!text;
    }catch(e){$i('thumb-ai-info').textContent=e.message;$i('thumb-ai-generate').disabled=true;}
  }
  $i('thumb-ai-workflow').onchange=()=>loadControls().catch(()=>{});
  $i('thumb-ai-context').onclick=()=>{
    const p=host.project(),d={...defaults,...host.document()?.thumbnail};
    const assetId=d.source_asset_id||p.assets.find(a=>a.kind==='video')?.id;
    const at=Number(d.at_ms)||0;
    const t=(p.transcripts||[]).find(t=>t.asset_id===assetId);
    if(!t){$i('thumb-ai-status').textContent='Nessuna trascrizione per questo media: scrivi la descrizione a mano.';return;}
    const segs=t.segments||[];
    const seg=segs.find(s=>s.start_ms<=at&&s.end_ms>at)||segs.reduce((best,s)=>!best||Math.abs(s.start_ms-at)<Math.abs(best.start_ms-at)?s:best,null);
    if(!seg){$i('thumb-ai-status').textContent='Nessun parlato vicino a questo tempo.';return;}
    $i('thumb-ai-prompt').value=`Contesto dal video: "${String(seg.text||'').trim()}"\nCopertina social: soggetto riconoscibile, espressione chiara, spazio per il titolo, alta leggibilità.`;
    $i('thumb-ai-status').textContent='Contesto inserito dalla trascrizione.';
  };
  async function payload(){
    const p=host.project(),d={...defaults,...host.document()?.thumbnail};
    if(!workflow||!controls?._text)throw Error('Scegli un workflow con input testo.');
    const assetId=d.source_asset_id||p.assets.find(a=>a.kind==='video')?.id;
    const asset=p.assets.find(a=>a.id===assetId);
    if(!asset)throw Error('Scegli una sorgente video per il frame.');
    const prompt=$i('thumb-ai-prompt').value.trim();
    if(!prompt)throw Error('Scrivi la descrizione della copertina.');
    const dur=asset.media?.duration_ms??0;
    const at=Math.max(0,Math.min(dur>0?dur-1:0,Number(d.at_ms)||0));
    const out={workflow_path:workflow.path,project_id:p.id,inputs:{[controls._text.address]:prompt},review_before_insert:true};
    if(controls._ref){
      // A real full-resolution frame asset, not the browser preview: the
      // workflow receives an image and the project keeps the frame reference.
      const frame=await host.ctx.api(`/api/projects/${p.id}/assets/${asset.id}/frame`,'POST',{at_ms:at});
      out.asset_inputs={[controls._ref.address]:frame.id};
    }
    return out;
  }
  details._payload=payload;
  $i('thumb-ai-generate').onclick=()=>host.run(async()=>{
    if(busy)return;busy=true;$i('thumb-ai-generate').disabled=true;
    try{
      const body=await payload();
      const job=await host.ctx.api('/api/comfy/submit','POST',body);
      await host.ctx.refreshJobs();
      let done;
      for(let i=0;i<1440;i++){
        done=await host.ctx.api('/api/jobs/'+job.id);
        $i('thumb-ai-status').textContent=done.message;
        if(['failed','succeeded'].includes(done.status))break;
        await new Promise(r=>setTimeout(r,500));
      }
      if(done.status!=='succeeded')throw Error(done.message||'Generazione non riuscita.');
      const ids=done.result?._nl_suite?.asset_ids||done.result?.asset_ids||done.result?.imported_asset_ids||[];
      const generated=ids[0];
      if(!generated){$i('thumb-ai-status').textContent='Generazione completata, ma nessun asset importato nel risultato.';return;}
      const d=host.document();d.thumbnail={...defaults,...d.thumbnail,source_asset_id:generated,at_ms:0};
      host.changed();await host.save();await host.ctx.reloadProject?.();
      refresh();
      $i('thumb-ai-status').textContent='Copertina generata e impostata come sorgente. Crea/aggiorna il PNG per applicare il titolo.';
      setTimeout(()=>document.querySelector('#social-thumb-render')?.click(),500);
    }finally{busy=false;$i('thumb-ai-generate').disabled=!controls?._text;}
  });
 }
 function show(j){const url=j.result?.url;if(!url)return;$('#social-thumb-image').src=url;$('#social-thumb-image').hidden=false;$('#social-thumb-download').href=url;$('#social-thumb-download').hidden=false;$('#social-thumb-status').textContent=j.message;}
 function refresh(){if(!host)return;const p=host.project(),d={...defaults,...host.document()?.thumbnail};for(const key of ['include_motion','include_captions']){const el=document.querySelector(`[data-thumb="${key}"]`);el.disabled=!!d.source_asset_id;}for(const el of document.querySelectorAll('[data-thumb]')){const k=el.dataset.thumb;if(k==='source_asset_id')el.innerHTML='<option value="">Montaggio corrente</option>'+(p?.assets||[]).filter(a=>['video','image'].includes(a.kind)).map(a=>`<option value="${a.id}">${a.name.replace(/[<>&"]/g,'')} · sorgente originale</option>`).join('');if(k==='font')el.innerHTML=window.MotionTypography.options(d.font);if(el.type==='checkbox')el.checked=d[k];else if(el.type==='number')el.value=Number((d[k]*(k==='at_ms'?.001:['font_size','x','y','title_width'].includes(k)?100:1)).toFixed(3));else el.value=d[k]||'';}if(projectId!==p?.id){projectId=p?.id;$('#social-thumb-image').hidden=true;$('#social-thumb-image').removeAttribute('src');$('#social-thumb-download').hidden=true;$('#social-thumb-status').textContent='';}$('#social-thumb-current').textContent=d.source_asset_id?'Usa questo fotogramma':'Usa il cursore del montaggio';previewSource(p,d);if(p&&!$('#social-thumb-image').getAttribute('src'))host.ctx.api('/api/jobs').then(jobs=>{const j=jobs.filter(j=>j.project_id===p.id&&j.status==='succeeded'&&j.result?.url?.includes('/social/thumbnail/')).sort((a,b)=>b.created_at.localeCompare(a.created_at))[0];if(j&&host.project()?.id===p.id&&!$('#social-thumb-image').getAttribute('src'))show(j);}).catch(()=>{});}
 async function previewSource(p,d){const asset=p?.assets.find(a=>a.id===d.source_asset_id),key=p?.id+':'+asset?.id;if(key===previewKey){const el=$('#thumb-source-video');if(asset?.kind==='video'&&el.readyState>=1&&Math.abs(el.currentTime-d.at_ms/1000)>.02)el.currentTime=d.at_ms/1000;return;}previewKey=key;const video=$('#thumb-source-video'),image=$('#thumb-source-image');video.pause();video.hidden=!asset||asset.kind!=='video';image.hidden=!asset||asset.kind!=='image';$('#thumb-source-note').textContent=asset?'Sorgente: scegli un fotogramma; Prepara PNG applica il titolo con il compositor.':'Il PNG usa il montaggio corrente. Usa il playhead per scegliere il punto.';if(!asset)return;if(asset.kind==='image')image.src=`/api/projects/${p.id}/assets/${asset.id}/file`;else{try{video.src=await MediaPreview.resolve(p.id,asset);video.onloadedmetadata=()=>{video.currentTime=d.at_ms/1000;};video.onseeked=()=>{if(host.project()?.id!==p.id||!host.document()?.thumbnail)return;host.document().thumbnail.at_ms=Math.round(video.currentTime*1000);const field=document.querySelector('[data-thumb="at_ms"]');field.value=video.currentTime.toFixed(3);host.changed();};}catch(e){$('#thumb-source-note').textContent=e.message;}}}
 return{configure,refresh};
})();
