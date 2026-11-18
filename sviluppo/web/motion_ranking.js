'use strict';
// A composition factory for the existing native Motion layers, not another renderer.
window.MotionRanking=(()=>{
 const $=q=>document.querySelector(q);
 function install(){
  $('#motion-preset').add(new Option('Classifica · ingressi animati','ranking'));
  const panel=document.createElement('div');panel.id='motion-ranking-fields';panel.hidden=true;
  panel.innerHTML=`<div class="lf-config"><label>Titolo della classifica<input id="ranking-title" value="La tua classifica"></label><label>Voci · una per riga<textarea id="ranking-rows" rows="5" placeholder="Nome | valore facoltativo"></textarea></label></div><p class="muted">Scrivi, per esempio, “Titolo del film | 9,5”. L’ordine delle righe è quello della classifica: nessun punteggio inventato o riordinato. Nomi, posizioni e valori diventano livelli testo separati.</p><details><summary>Stile e tempi · facoltativi</summary><div class="lf-config"><label>Durata per pagina (s)<input id="ranking-duration" type="number" min="2" max="30" step=".5" value="6"></label><label>Comparsa delle voci<select id="ranking-order"><option value="countdown">Dal fondo al primo posto</option><option value="forward">Dal primo all’ultimo posto</option></select></label><label>Colore di accento<input id="ranking-accent" type="color" value="#7531ff"></label><label>Colore dei valori<input id="ranking-value-color" type="color" value="#4fe5e8"></label><label>Font<select id="ranking-font">${window.MotionTypography.options('Segoe UI')}</select></label></div></details><p class="muted">Scorrimento breve, dissolvenze e ingressi sfalsati. Oltre quattro voci vengono create pagine consecutive: ogni pagina resta entro i 20 livelli del renderer e conserva i numeri originali. Crea, prepara l’anteprima e approva prima di esportare. Non richiede IA né ComfyUI.</p>`;
  $('#motion-title-copy-fields').after(panel);
  $('#motion-preset').addEventListener('change',()=>{panel.hidden=$('#motion-preset').value!=='ranking';if(!panel.hidden){const font=$('#ranking-font');font.innerHTML=window.MotionTypography.options(font.value);}});
 }
 function read(){
  const title=$('#ranking-title').value.trim(),duration=Number($('#ranking-duration').value)*1000;
  const rows=$('#ranking-rows').value.split(/\r?\n/).map(s=>s.trim()).filter(Boolean).map(s=>{const i=s.indexOf('|');return{label:(i<0?s:s.slice(0,i)).trim(),value:i<0?'':s.slice(i+1).trim()};});
  if(!title)throw Error('Scrivi un titolo per la classifica.');
  if(!rows.length||rows.some(r=>!r.label))throw Error('Inserisci le voci della classifica, una per riga, con un nome prima del separatore |.');
  if(!Number.isFinite(duration)||duration<2000||duration>30000)throw Error('La durata di una pagina deve essere fra 2 e 30 secondi (limite del renderer Motion).');
  return{title,rows,duration_ms:Math.round(duration),countdown:$('#ranking-order').value==='countdown',accent:$('#ranking-accent').value,value_color:$('#ranking-value-color').value,font:$('#ranking-font').value};
 }
 function build(config,{makeLayer,pose,at,aspect}){
  const portrait=aspect.split(':').map(Number).reduce((a,b)=>a/b)<1;
  const center=portrait?.5:.34,width=portrait?.84:.56,left=center-width/2;
  const pages=[];
  for(let offset=0;offset<config.rows.length;offset+=4){
   const rows=config.rows.slice(offset,offset+4),duration=config.duration_ms;
   const g={id:crypto.randomUUID(),name:config.title+(config.rows.length>4?` · ${offset+1}–${offset+rows.length}`:''),start_ms:at+pages.length*duration,duration_ms:duration,anchor_clip_id:null,anchor_source_ms:0,approved:false,evidence:'Classifica manuale · ordine e valori forniti dall’utente',layers:[]};
   function add(kind,text,name,x,y,w,h,size,color,delay=0,maxOpacity=1){
    const l=makeLayer(kind,text,x,y,color,w,h,size);l.name=name;l.font=config.font;
    l.keyframes=[pose(x-.025,y,0,1),...(delay?[pose(x-.025,y,0,1,delay)]:[]),pose(x,y,maxOpacity,1,delay+350),pose(x,y,maxOpacity,1,duration-350),pose(x+.012,y,0,1,duration)];
    g.layers.push(l);return l;
   }
   add('rect','','Fondo titolo',center,.19,width,.112,.02,'#151121',0,.97);
   const title=add('text',config.title,'Titolo classifica',center,.19,width*.9,.09,portrait?.028:.045,'#FFFFFF');title.decoration.bold=true;
   add('rect','','Linea di accento',center,.265,width,.004,.02,config.accent);
   rows.forEach((r,i)=>{
    const rank=offset+i+1,y=.36+i*.135,reveal=config.countdown?rows.length-1-i:i,delay=180+reveal*Math.min(350,(duration-1200)/rows.length);
    add('rect','',`Fondo posizione ${rank}`,center,y,width,.112,.02,'#151121',Math.round(delay),.94);
    const n=add('text',String(rank).padStart(2,'0'),`Posizione ${rank}`,left+width*.09,y,width*.13,.075,portrait?.026:.041,'#FFFFFF',Math.round(delay));Object.assign(n.decoration,{bold:true,glow_color:config.accent,glow_radius:.002,glow_opacity:.5});
    const label=add('text',r.label,`Voce ${rank}`,left+width*.48,y,width*.57,.1,portrait?.021:.034,'#FFFFFF',Math.round(delay));label.decoration.bold=true;
    if(r.value){const value=add('text',r.value,`Valore ${rank}`,left+width*.87,y,width*.18,.075,portrait?.022:.036,config.value_color,Math.round(delay));value.decoration.bold=true;}
   });
   pages.push(g);
  }
  return pages;
 }
 return{install,read,build};
})();
