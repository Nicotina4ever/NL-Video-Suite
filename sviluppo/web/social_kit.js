'use strict';
// Social 9:16 kit for the classic studio: one-click subject reframe (CPU),
// blurred-background fallback, and platform safe masks on the program monitor.
// It reuses the existing camera inspector (analyze -> proposal -> adopt) and the
// inspector's own guarded apply, so no new write path is introduced.
window.SocialKit=(()=>{
  const $=s=>document.querySelector(s);
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const PRESETS={tiktok:{top:10,bottom:18,right:12,label:'TikTok'},reels:{top:12,bottom:22,right:14,label:'Reels'},shorts:{top:8,bottom:16,right:12,label:'Shorts'},none:{top:0,bottom:0,right:0,label:'Nessuna'}};
  let preset=localStorage.getItem('nl-social-mask')||'tiktok';
  let busy=false,started=false,selected=null,styleInstalled=false;

  function installStyle(){
    if(styleInstalled||$('#social-kit-style'))return;styleInstalled=true;
    const style=document.createElement('style');style.id='social-kit-style';style.textContent=`
#social-safe{position:absolute;inset:0;pointer-events:none;display:none;border-radius:6px;overflow:hidden;z-index:6}
#social-safe.on{display:block}
#social-safe .ss-action{position:absolute;inset:5%;border:1px dashed rgba(255,255,255,.5)}
#social-safe .ss-title{position:absolute;inset:10%;border:1px dashed rgba(139,228,230,.8)}
#social-safe .ss-top{position:absolute;left:0;right:0;top:0;height:var(--ss-top,10%);background:rgba(0,0,0,.38);border-bottom:1px dashed #ff6b6b}
#social-safe .ss-bottom{position:absolute;left:0;right:0;bottom:0;height:var(--ss-bottom,18%);background:rgba(0,0,0,.38);border-top:1px dashed #ff6b6b}
#social-safe .ss-right{position:absolute;right:0;top:var(--ss-top,10%);bottom:var(--ss-bottom,18%);width:var(--ss-right,12%);background:rgba(0,0,0,.32);border-left:1px dashed #ff6b6b}
#social-safe .ss-caption{position:absolute;left:8px;bottom:calc(var(--ss-bottom,18%) + 6px);font:10px Consolas,monospace;color:#ffd27a;text-shadow:0 1px 2px #000}
#social-kit{border:1px solid var(--line);border-radius:8px;padding:10px;margin:0 0 10px;background:var(--panel)}
#social-kit h3{margin:0 0 6px;font-size:14px}
#social-kit .social-row{display:flex;gap:6px;flex-wrap:wrap;margin:6px 0}
#social-kit button{padding:5px 9px}
#nl-social-status{margin:4px 0 0;font-size:12px}
`;document.head.append(style);
  }

  function overlay(){
    const frame=$('#program-frame');if(!frame)return null;
    let el=frame.querySelector('#social-safe');
    if(!el){el=document.createElement('div');el.id='social-safe';el.innerHTML='<div class="ss-action"></div><div class="ss-title"></div><div class="ss-top"></div><div class="ss-bottom"></div><div class="ss-right"></div><div class="ss-caption">MASCHERA SOCIAL · <span id="ss-name"></span></div>';frame.append(el);}
    return el;
  }
  function applyMask(){
    const el=overlay();if(!el)return;
    const z=PRESETS[preset]||PRESETS.tiktok;
    el.dataset.preset=preset;el.classList.toggle('on',preset!=='none');
    el.style.setProperty('--ss-top',z.top+'%');el.style.setProperty('--ss-bottom',z.bottom+'%');el.style.setProperty('--ss-right',z.right+'%');
    const name=el.querySelector('#ss-name');if(name)name.textContent=z.label;
    positionOverlay();
  }
  // The panel is wider than the 9:16 composition: the guides must sit on the
  // canvas, not on the black bars around it.
  function positionOverlay(){
    const frame=$('#program-frame'),canvas=$('#program-canvas'),el=overlay();
    if(!frame||!canvas||!el)return;
    const f=frame.getBoundingClientRect(),c=canvas.getBoundingClientRect();
    if(c.width<2||c.height<2)return;
    el.style.left=(c.left-f.left)+'px';el.style.top=(c.top-f.top)+'px';
    el.style.width=c.width+'px';el.style.height=c.height+'px';
    el.style.right='auto';el.style.bottom='auto';
  }
  function watchGeometry(){
    if(watchGeometry.done)return;watchGeometry.done=true;
    window.addEventListener('resize',positionOverlay);
    const frame=$('#program-frame'),canvas=$('#program-canvas');
    if(window.ResizeObserver){
      if(frame)new ResizeObserver(positionOverlay).observe(frame);
      if(canvas)new ResizeObserver(positionOverlay).observe(canvas);
    }
  }
  function status(message,error=false){
    const p=$('#nl-social-status');if(p){p.textContent=message;p.style.color=error?'var(--bad)':'';}
    if(error&&window.notice)try{notice(message,true);}catch{}
  }

  function ensurePanel(){
    installStyle();
    const insp=$('#clip-inspector');if(!insp)return false;
    if(!insp.querySelector('#social-kit')){
      const box=document.createElement('section');
      box.id='social-kit';
      box.innerHTML=`<h3>Social 9:16 · reframe e maschere</h3>
        <div class="social-row"><button type="button" id="social-detect">Rileva soggetto e adatta</button><button type="button" id="social-blur" class="secondary">Sfondo sfocato</button></div>
        <label>Maschera monitor <select id="social-mask"><option value="tiktok">TikTok</option><option value="reels">Reels</option><option value="shorts">Shorts</option><option value="none">Nessuna</option></select></label>
        <p id="nl-social-status" class="muted" role="status">Seleziona un clip video, poi «Rileva soggetto e adatta».</p>`;
      const head=insp.querySelector('.actions');
      if(head&&head.nextSibling)insp.insertBefore(box,head.nextSibling);else insp.prepend(box);
      const mask=box.querySelector('#social-mask');mask.value=preset;
      mask.onchange=()=>{preset=mask.value;try{localStorage.setItem('nl-social-mask',preset);}catch{}applyMask();status(`Maschera ${(PRESETS[preset]||{}).label||preset}: aree UI della piattaforma evidenziate.`);};
      box.querySelector('#social-detect').onclick=detect;
      box.querySelector('#social-blur').onclick=blurFallback;
    }
    applyMask();
    return true;
  }

  async function detect(){
    if(busy)return;
    const analyze=$('#camera-analyze'),adopt=$('#camera-adopt');
    if(!analyze||!adopt){status('Sezione camera non disponibile: apri un clip e ricarica.',true);return;}
    if(!selected){status('Seleziona prima un clip video nella timeline.',true);return;}
    busy=true;started=false;
    const detectBtn=$('#social-detect');if(detectBtn)detectBtn.disabled=true;
    try{
      const behavior=$('#camera-behavior');if(behavior&&behavior.value!=='composed')behavior.value='composed';
      status('Analisi CPU del soggetto in corso… (nessuna GPU, nessun invio esterno)');
      analyze.click();
      const deadline=Date.now()+15*60*1000;
      await sleep(400);
      while(!adopt.hidden&&Date.now()<deadline){
        const line=$('#camera-status')?.textContent||'';
        if(line)status(line);
        if(!analyze.disabled)break; // handler finished
        await sleep(700);
      }
      if(adopt.hidden){const line=$('#camera-status')?.textContent||'Analisi terminata senza proposta.';throw Error(line);}
      adopt.click();
      await sleep(1400);
      const finalLine=$('#camera-status')?.textContent||'';
      const blur=(finalLine.indexOf('sfondo sfocato')>=0);
      status((blur?'Nessun soggetto stabile: applicato 16:9 su sfondo sfocato. ':'Soggetto agganciato: crop 9:16 applicato. ')+(finalLine||'Controlla il monitor e il playback.'));
    }catch(e){status(e.message||String(e),true);}
    finally{busy=false;if(detectBtn)detectBtn.disabled=false;}
  }

  function blurFallback(){
    const fit=$('#clip-fit'),apply=$('#inspector-apply');
    if(!fit||!apply){status('Ispettore non pronto: seleziona un clip.',true);return;}
    if(!selected){status('Seleziona prima un clip video nella timeline.',true);return;}
    window.__nlSocialBlur=true;
    fit.value='blur';fit.dispatchEvent(new Event('change',{bubbles:true}));
    apply.click();
    const line='Sfondo sfocato applicato: il 16:9 riempie il 9:16 con sfondo ingrandito e sfocato.';
    status(line);
    setTimeout(()=>status(line),1600);
  }

  function attach(){
    installStyle();
    watchGeometry();
    document.addEventListener('nl:clip-selection',e=>{
      selected=e?.detail?.clip_id||null;
      const p=$('#nl-social-status');
      if(p&&!busy)p.textContent=selected?'Clip pronto: rileva il soggetto o applica lo sfondo sfocato.':'Seleziona un clip video nella timeline.';
    });
    // The inspector is built by nle.js before this bundle runs; retry once if a
    // future load order changes.
    if(!ensurePanel()){let tries=0;const timer=setInterval(()=>{if(ensurePanel()||++tries>20)clearInterval(timer);},250);}
    applyMask();
    window.dispatchEvent(new Event('resize'));
    setTimeout(positionOverlay,300);
    setTimeout(positionOverlay,1500);
  }
  return{attach,applyMask,detect,blurFallback};
})();
window.addEventListener('DOMContentLoaded',()=>window.SocialKit.attach());
