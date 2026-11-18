'use strict';
// Four work areas over the existing editors. Nodes/state are moved, not copied.
window.WorkflowNavigation=(()=>{
 const $=s=>document.querySelector(s),groups={materials:['projects'],edit:['studio','longform','subtitles','motion','graphics'],social:['shorts','social'],story:['story','workflows'],tools:['jobs','system','storage']};
 const labels={materials:'Progetti e materiali',edit:'Montaggio',social:'Shorts e Social',story:'Storie e Regia AI',tools:'Studio e strumenti'},homes={materials:'projects',edit:'studio',social:'shorts',story:'story',tools:'system'};
 let hooks,bar,library,home,buttons;
 function owner(tab){return Object.keys(groups).find(g=>groups[g].includes(tab))||'edit';}
 function configure(h){hooks=h;const rail=$('.rail'),bottom=$('.rail-bottom');library=$('.edit-grid>.library');home=document.createComment('shared-material-library');library.before(home);
  const page=document.createElement('section');page.id='tab-projects';page.className='tab';page.innerHTML='<section class="panel"><p class="eyebrow">PROGETTO · MATERIALI ORIGINALI AL SICURO</p><h2>Progetti e materiali</h2><p>Importa una copia di video, fotografie e audio. Sono gli stessi materiali del montaggio e delle scene, non una seconda libreria.</p><div class="actions"><button data-route="studio">Monta manualmente</button><button data-route="longform" class="secondary">Trascrivi o sgrossa</button><button data-route="shorts" class="secondary">Prepara shorts</button><button data-route="story" class="secondary">Sviluppa una storia</button></div></section><div id="project-materials-slot"></div>';$('main').append(page);
  const projectButton=document.createElement('button');projectButton.dataset.tab='projects';projectButton.textContent='Materiali';projectButton.onclick=()=>open('projects');
  bar=document.createElement('nav');bar.id='workflow-tools';bar.setAttribute('aria-label','Strumenti del lavoro corrente');buttons=[projectButton,...document.querySelectorAll('.rail [data-tab]')];
  const names={studio:'Montaggio',longform:'Trascrizione e tagli',subtitles:'Sottotitoli',motion:'Titoli e motion',graphics:'Grafiche dal parlato',shorts:'Clip',social:'Copertine e consegna',pipeline:'Pipeline ClipsAI · avanzata',story:'Storia, reference e scene',workflows:'Workflow e riparazioni',jobs:'Lavori e recupero',system:'Provider e modelli',storage:'Spazio e pulizia'};
  groups.edit.push('pipeline');for(const b of buttons){b.textContent=names[b.dataset.tab]||b.textContent;bar.append(b);}$('.projectbar').after(bar);
  for(const g of ['materials','edit','social','story','tools']){const b=document.createElement('button');b.dataset.workflow=g;b.textContent=labels[g];b.type='button';b.onclick=()=>open(homes[g]);if(g==='tools')b.className='workflow-utility';rail.insertBefore(b,bottom);}
  const audio=document.createElement('button');audio.id='workflow-audio';audio.textContent='Audio e musica';audio.onclick=()=>{open('studio');$('#nle-ai-audio')?.click();};bar.append(audio);
  const exp=document.createElement('button');exp.id='workflow-export';exp.textContent='Esporta…';exp.onclick=()=>window.ExportControls.open();bar.append(exp);
  document.addEventListener('click',e=>{const route=e.target.closest('[data-route]');if(route){open(route.dataset.route);if(route.dataset.focus){const target=$(route.dataset.focus);if(target?.tagName==='DETAILS')target.open=true;target?.scrollIntoView({block:'start',behavior:'smooth'});target?.focus();}}});
  document.addEventListener('keydown',e=>{if(!e.altKey||e.ctrlKey||e.metaKey||!['1','2','3','4'].includes(e.key))return;e.preventDefault();open(homes[['materials','edit','social','story'][Number(e.key)-1]]);});
  // Creative sequencing is contextual; its existing toolbar button opens the same panel.
  $('#tab-studio').append($('#montage-studio'));
  $('#nle-new-object').textContent='+ Segnaposto';$('#nle-new-object').title='Oggetto generativo vuoto, da descrivere e revisionare';
  if($('#nle-ai-graphic'))$('#nle-ai-graphic').textContent='Genera grafica';if($('#nle-ai-audio'))$('#nle-ai-audio').textContent='Audio e musica';if($('#nle-ai-frame'))$('#nle-ai-frame').textContent='Anima un frame';
  // Shortcuts fix the former dead-end instructions; no mandatory wizard or ASR.

  for(const tab of ['longform','subtitles','motion','social','graphics']){const actions=document.createElement('div');actions.className='context-return';actions.innerHTML='<button data-route="studio" class="secondary">← Timeline</button>';$('#tab-'+tab).prepend(actions);}
  const extra=document.createElement('details');extra.id='workflow-advanced';extra.innerHTML='<summary>Altri strumenti</summary><div class="actions"></div>';bar.append(extra);for(const id of ['subtitles','graphics','pipeline']){const b=buttons.find(b=>b.dataset.tab===id);if(b)extra.lastElementChild.append(b);}extra.lastElementChild.append(audio);
  sync('studio');window.dispatchEvent(new Event('resize'));
 }
 function sync(tab){if(!hooks)return;const group=owner(tab);for(const b of buttons){const show=['edit','social'].includes(group)?['studio','longform','shorts','social','motion','subtitles','graphics','pipeline'].includes(b.dataset.tab):groups[group].includes(b.dataset.tab);b.hidden=!show;b.classList.toggle('active',b.dataset.tab===tab);if(b.dataset.tab===tab)b.setAttribute('aria-current','page');else b.removeAttribute('aria-current');}for(const b of document.querySelectorAll('.rail [data-workflow]')){b.classList.toggle('active',b.dataset.workflow===group);b.setAttribute('aria-pressed',String(b.dataset.workflow===group));}
  $('#workflow-advanced').hidden=!['edit','social'].includes(group);if(['subtitles','graphics','pipeline'].includes(tab))$('#workflow-advanced').open=true;$('#workflow-audio').hidden=group!=='edit';$('#workflow-export').hidden=['materials','tools'].includes(group);document.body.dataset.workflow=group;
  if(tab==='projects')$('#project-materials-slot').append(library);else if(library.parentElement?.id==='project-materials-slot')home.after(library);
  // Keep each editor's scroll and draft in place. The creative rail stays short.
 }
 function open(tab){hooks.showTab(tab);}
 return{configure,sync,open,owner};
})();
