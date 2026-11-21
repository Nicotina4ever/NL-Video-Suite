'use strict';
// Keyboard-first command palette for the classic studio: every visible action
// of the current tab plus navigation and the main workspace controls, filtered
// by text and run with Enter. No duplicated state: it clicks the real control.
window.CommandPalette=(()=>{
  const $=s=>document.querySelector(s);
  let overlay=null,input=null,list=null,items=[];

  function installStyle(){
    if($('#command-palette-style'))return;
    const style=document.createElement('style');style.id='command-palette-style';style.textContent=`
#command-palette{position:fixed;inset:0;z-index:120;display:flex;align-items:flex-start;justify-content:center;padding-top:12vh;background:#05030ab8;backdrop-filter:blur(2px)}
#command-palette[hidden]{display:none}
#command-palette .cp-box{width:min(620px,92vw);max-height:70vh;display:flex;flex-direction:column;gap:6px;padding:12px;border:1px solid var(--line);border-radius:12px;background:var(--panel);box-shadow:0 24px 70px #000a}
#command-palette input{width:100%;font-size:15px;padding:10px}
#command-palette .cp-list{overflow:auto;display:flex;flex-direction:column;gap:2px}
#command-palette button.cp-item{display:flex;justify-content:space-between;gap:12px;text-align:left;background:transparent;border:1px solid transparent;border-radius:7px;padding:8px 10px}
#command-palette button.cp-item.active,#command-palette button.cp-item:hover{border-color:var(--accent);background:#7531ff22}
#command-palette kbd{font:11px Consolas,monospace;color:var(--muted)}
#command-palette .cp-empty{padding:12px;color:var(--muted)}
`;document.head.append(style);
  }

  function collect(){
    const out=[],seen=new Set();
    const add=(el,group)=>{
      if(!el||el.disabled)return;
      if(el.closest('[hidden]'))return;
      if(el.getAttribute('aria-hidden')==='true')return;
      const label=(el.getAttribute('aria-label')||el.title||el.textContent||'').replace(/\s+/g,' ').trim();
      if(!label||label.length>120)return;
      const key=label.toLowerCase();
      if(seen.has(key))return;seen.add(key);
      out.push({label,title:(el.title||'').replace(/\s+/g,' ').trim(),group,run:()=>el.click()});
    };
    const active=$('.tab.active');
    if(active)active.querySelectorAll('button,a[download],[data-thumb],.lf-toolbar button').forEach(el=>add(el,'Pannello'));
    document.querySelectorAll('#workflow-tools button,.workspace-controls button,.projectbar button,.projectbar a').forEach(el=>add(el,'Barra'));
    document.querySelectorAll('.rail [data-tab]').forEach(el=>add(el,'Vai a'));
    return out;
  }

  function render(){
    const query=(input.value||'').toLowerCase().trim();
    const source=collect();
    items=source.filter(c=>!query||c.label.toLowerCase().includes(query)||c.title.toLowerCase().includes(query)).slice(0,40);
    if(!items.length){list.innerHTML='<p class="cp-empty">Nessun comando corrisponde.</p>';return;}
    list.innerHTML=items.map((c,i)=>`<button type="button" class="cp-item ${i===0?'active':''}" data-cp="${i}"><span>${escapeHTML(c.label)}${c.group?` <small style="color:var(--muted)">- ${escapeHTML(c.group)}</small>`:''}</span>${c.title?`<kbd>${escapeHTML(c.title)}</kbd>`:''}</button>`).join('');
    list.querySelectorAll('[data-cp]').forEach(btn=>btn.addEventListener('click',()=>run(Number(btn.dataset.cp))));
  }
  function escapeHTML(v){return String(v??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
  function run(index){
    const item=items[index];if(!item)return;
    close();
    try{item.run();}catch(error){if(window.notice)notice(error.message,true);}
  }
  function open(){
    installStyle();
    if(!overlay){
      overlay=document.createElement('div');overlay.id='command-palette';overlay.hidden=true;
      overlay.innerHTML='<div class="cp-box" role="dialog" aria-label="Palette comandi"><input id="cp-input" type="search" placeholder="Cerca un comando o un pannello…" autocomplete="off"><div class="cp-list"></div><p style="margin:0;color:var(--muted);font-size:11px">Invio esegue · Esc chiude · Shift+6 mixer · Ctrl+Shift+P comandi</p></div>';
      document.body.append(overlay);
      input=overlay.querySelector('#cp-input');list=overlay.querySelector('.cp-list');
      overlay.addEventListener('click',e=>{if(e.target===overlay)close();});
      input.addEventListener('input',render);
      input.addEventListener('keydown',e=>{
        if(e.key==='Escape'){e.preventDefault();close();}
        else if(e.key==='Enter'){e.preventDefault();run(0);}
        else if(e.key==='ArrowDown'||e.key==='ArrowUp'){
          e.preventDefault();
          const buttons=[...list.querySelectorAll('.cp-item')];if(!buttons.length)return;
          const active=buttons.findIndex(b=>b.classList.contains('active'));
          const next=e.key==='ArrowDown'?Math.min(buttons.length-1,active+1):Math.max(0,active-1);
          buttons.forEach((b,i)=>b.classList.toggle('active',i===next));
          buttons[next].scrollIntoView({block:'nearest'});
        }
      });
    }
    overlay.hidden=false;input.value='';render();setTimeout(()=>input.focus(),0);
  }
  function close(){if(overlay)overlay.hidden=true;}

  function attach(){
    document.addEventListener('keydown',e=>{
      if(e.ctrlKey&&e.shiftKey&&!e.altKey&&String(e.key).toLowerCase()==='p'){e.preventDefault();open();return;}
      if(e.key==='Escape'&&overlay&&!overlay.hidden){e.preventDefault();close();}
    });
    const bar=document.querySelector('#tab-studio .workspace-controls');
    if(bar&&!document.querySelector('#command-palette-open')){
      const b=document.createElement('button');b.id='command-palette-open';b.type='button';b.className='secondary';b.textContent='⌘ Comandi';b.title='Palette comandi (Ctrl+Shift+P)';
      b.onclick=open;bar.prepend(b);
    }
  }
  if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',attach);else attach();
  return{open,close,collect};
})();
