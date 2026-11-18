'use strict';
// UI preference only. Never touches project graphics, canvas pixels or render settings.
window.NLAppearance=(()=>{
 const media=matchMedia('(prefers-color-scheme: dark)'),key='nl-suite.appearance.v1';let preference='dark';
 try{const saved=localStorage.getItem(key);if(['dark','light','system'].includes(saved))preference=saved;}catch{}
 function apply(){const actual=preference==='system'?(media.matches?'dark':'light'):preference;document.documentElement.dataset.theme=actual;document.documentElement.style.colorScheme=actual;const select=document.querySelector('#appearance-theme');if(select)select.value=preference;}
 function choose(value){if(!['dark','light','system'].includes(value))return;preference=value;try{localStorage.setItem(key,value);}catch{}apply();}
 media.addEventListener('change',apply);apply();
 document.addEventListener('DOMContentLoaded',()=>{const panel=document.createElement('section');panel.className='panel';panel.innerHTML='<h2>Aspetto</h2><label>Tema dell’applicazione<select id="appearance-theme"><option value="dark">Scuro</option><option value="light">Chiaro</option><option value="system">Sistema · segue Windows</option></select></label><p class="muted">Palette UI dal LogoMio.svg: viola #7531FE, magenta #E4469B, lilla #AD85C7. Non cambia colori o font dei video e non applica loghi ai progetti.</p>';document.querySelector('#tab-system').prepend(panel);panel.querySelector('select').addEventListener('change',e=>choose(e.target.value));apply();});
 return{choose,preference:()=>preference};
})();
