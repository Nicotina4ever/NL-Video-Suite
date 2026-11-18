'use strict';
// SVG is parsed as inert XML and converted to bounded numeric geometry, never injected into DOM.
window.MotionSVG=(()=>{
 const number=/[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:e[-+]?\d+)?/ig;
 function numbers(value){const values=String(value||'').match(number)||[];if(String(value||'').replace(number,'').replace(/[\s,]/g,''))throw Error('Numeri SVG non validi.');return values.map(Number);}
 const identity=[1,0,0,1,0,0],mul=(a,b)=>[a[0]*b[0]+a[2]*b[1],a[1]*b[0]+a[3]*b[1],a[0]*b[2]+a[2]*b[3],a[1]*b[2]+a[3]*b[3],a[0]*b[4]+a[2]*b[5]+a[4],a[1]*b[4]+a[3]*b[5]+a[5]];
 function transform(value){let m=identity.slice(),rest=value||'';const re=/([a-z]+)\s*\(([^)]*)\)/ig;for(const match of rest.matchAll(re)){const n=numbers(match[2]);let b;switch(match[1]){case'matrix':if(n.length!==6)throw Error('matrix SVG:6 valori.');b=n;break;case'translate':if(n.length<1||n.length>2)throw Error('translate SVG non valido.');b=[1,0,0,1,n[0],n[1]||0];break;case'scale':if(n.length<1||n.length>2)throw Error('scale SVG non valido.');b=[n[0],0,0,n[1]??n[0],0,0];break;case'rotate':{if(![1,3].includes(n.length))throw Error('rotate SVG non valido.');const a=n[0]*Math.PI/180,c=Math.cos(a),s=Math.sin(a);b=[c,s,-s,c,0,0];if(n.length===3)b=mul(mul([1,0,0,1,n[1],n[2]],b),[1,0,0,1,-n[1],-n[2]]);break;}default:throw Error('Trasformazione SVG non supportata: '+match[1]);}m=mul(m,b);}if(rest.replace(re,'').trim())throw Error('Trasformazione SVG non valida.');return m;}
 function path(value){
  const tokens=String(value).match(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:e[-+]?\d+)?/ig)||[];
  if(String(value).replace(/[a-z]|[-+]?(?:\d*\.\d+|\d+\.?\d*)(?:e[-+]?\d+)?/ig,'').replace(/[\s,]/g,''))throw Error('Percorso SVG non valido.');
  let i=0,command='',current=[0,0],origin=[0,0],control=null,previous='',out=[];
  const take=n=>{if(i+n>tokens.length||tokens.slice(i,i+n).some(v=>/^[a-z]$/i.test(v)))throw Error('Percorso SVG incompleto.');return tokens.slice(i,i+=n).map(Number);};
  while(i<tokens.length){if(/^[a-z]$/i.test(tokens[i]))command=tokens[i++];if(!command)throw Error('Percorso SVG senza comando.');const op=command.toUpperCase(),relative=command!==op,base=current.slice(),point=(x,y)=>[x+(relative?base[0]:0),y+(relative?base[1]:0)];let points,n;
   if(op==='Z'){out.push({op:'Z',points:[]});current=origin.slice();control=null;previous=op;command='';continue;}
   if(!'MLHVCSQT'.includes(op))throw Error('Comando SVG '+op+' non supportato: converti archi in curve cubiche nell’editor vettoriale.');
   if(op==='M'||op==='L'){n=take(2);points=[point(...n)];out.push({op,points});current=points[0];if(op==='M'){origin=current.slice();command=relative?'l':'L';}}
   if(op==='H'||op==='V'){n=take(1);current=op==='H'?[n[0]+(relative?base[0]:0),base[1]]:[base[0],n[0]+(relative?base[1]:0)];out.push({op:'L',points:[current]});}
   if(op==='C'||op==='S'){n=take(op==='C'?6:4);const first=op==='C'?point(n.shift(),n.shift()):(['C','S'].includes(previous)&&control?[2*base[0]-control[0],2*base[1]-control[1]]:base);points=[first,point(n[0],n[1]),point(n[2],n[3])];out.push({op:'C',points});control=points[1];current=points[2];}
   if(op==='Q'||op==='T'){n=take(op==='Q'?4:2);const q=op==='Q'?point(n.shift(),n.shift()):(['Q','T'].includes(previous)&&control?[2*base[0]-control[0],2*base[1]-control[1]]:base),end=point(n[0],n[1]);points=[[base[0]+2/3*(q[0]-base[0]),base[1]+2/3*(q[1]-base[1])],[end[0]+2/3*(q[0]-end[0]),end[1]+2/3*(q[1]-end[1])],end];out.push({op:'C',points});control=q;current=end;}
   if(!['C','S','Q','T'].includes(op))control=null;previous=op;if(out.length>4096)throw Error('Percorso SVG oltre4096 comandi.');
  }return out;
 }
 function parse(xml){
  if(new TextEncoder().encode(xml).length>2*1024*1024)throw Error('SVG oltre2MiB.');
  if(/<!DOCTYPE|<!ENTITY|<\?/i.test(xml.replace(/^\s*<\?xml\s[^?]*\?>/,'')))throw Error('DTD, entità e istruzioni XML non consentite.');
  const doc=new DOMParser().parseFromString(xml,'image/svg+xml'),root=doc.documentElement;
  if(root.localName!=='svg'||doc.querySelector('parsererror'))throw Error('Documento SVG non valido.');
  const v=numbers(root.getAttribute('viewBox'));
  if(v.length!==4||v.some(n=>!Number.isFinite(n))||v[2]<=0||v[3]<=0)throw Error('SVG:serve un viewBox numerico valido.');
  let nodes=0;const shapes=[];
  function walk(el,m,paint,opacity,depth){
   if(++nodes>10000||depth>24)throw Error('SVG troppo complesso.');
   const tag=el.localName;if(!['svg','g','path','rect','polygon','polyline','circle','ellipse','title','desc'].includes(tag))throw Error('SVG '+tag+' non supportato. Converti testo/tratti/effetti in tracciati; usa PNG per texture.');
   const p={...paint};let local=1;
   for(const a of el.attributes){const key=a.name,val=a.value;if(key.startsWith('data-')||key==='xmlns'||key==='xmlns:xlink'||key==='id'||key==='version'||key==='viewBox'||key==='width'||key==='height'||key==='transform'||['d','x','y','rx','ry','cx','cy','r','points'].includes(key))continue;if(key==='style'){for(const entry of val.split(';').filter(e=>e.trim())){const split=entry.split(':');if(split.length!==2)throw Error('Stile SVG non valido.');const k=split[0].trim(),value=split[1].trim();if(k==='opacity')local=Number(value);else if(['fill','fill-opacity','fill-rule','stroke'].includes(k))p[k]=value;else throw Error('Stile SVG non supportato: '+k);}continue;}if(key==='opacity')local=Number(val);else if(['fill','fill-opacity','fill-rule','stroke'].includes(key))p[key]=val;else throw Error('Attributo SVG non supportato: '+key);}
   if(!Number.isFinite(local)||local<0||local>1)throw Error('Opacità SVG non valida.');opacity*=local;m=mul(m,transform(el.getAttribute('transform')));
   if(tag==='title'||tag==='desc')return;
   if(tag==='svg'||tag==='g'){if(tag==='svg'&&depth>0)throw Error('Viewport SVG annidato non supportato.');for(const child of el.children)walk(child,m,p,opacity,depth+1);return;}
   if(p.stroke&&p.stroke!=='none')throw Error('SVG con stroke: espandi il tratto in tracciato prima dell’importazione.');
   if(p['fill-rule']&&p['fill-rule']!=='nonzero')throw Error('SVG evenodd:converti in tracciati nonzero per mantenere i fori.');
   if(p.fill==='none')return;const fill=p.fill||'#000000',aliases={black:'#000000',white:'#FFFFFF',red:'#FF0000',blue:'#0000FF',none:null};let color=aliases[fill.toLowerCase()]||fill;if(/^#[0-9a-f]{3}$/i.test(color))color='#'+color.slice(1).split('').map(c=>c+c).join('');if(!/^#[0-9a-f]{6}$/i.test(color))throw Error('SVG:colore pieno #RRGGBB richiesto (niente URL/gradienti).');
   const alpha=Number(p['fill-opacity']??1);if(!Number.isFinite(alpha)||alpha<0||alpha>1)throw Error('fill-opacity SVG non valida.');
   const get=(k,d=0)=>{const n=Number(el.getAttribute(k)??d);if(!Number.isFinite(n))throw Error('Dimensione SVG non numerica.');return n;};let commands;
   if(tag==='path')commands=path(el.getAttribute('d')||'');
   else if(tag==='rect'){if(get('rx')||get('ry'))throw Error('Rettangolo arrotondato:converti in tracciato.');const x=get('x'),y=get('y'),w=get('width'),h=get('height');if(w<=0||h<=0)return;commands=path(`M${x} ${y}h${w}v${h}h${-w}Z`);}
   else if(tag==='polygon'||tag==='polyline'){const n=numbers(el.getAttribute('points'));if(n.length<4||n.length%2)throw Error('Punti SVG non validi.');commands=path('M'+n.slice(0,2).join(' ')+'L'+n.slice(2).join(' ')+'Z');}
   else {const x=get('cx'),y=get('cy'),rx=get(tag==='circle'?'r':'rx'),ry=get(tag==='circle'?'r':'ry'),c=.5522847498307936;if(rx<=0||ry<=0)return;commands=path(`M${x+rx} ${y}C${x+rx} ${y+c*ry} ${x+c*rx} ${y+ry} ${x} ${y+ry}C${x-c*rx} ${y+ry} ${x-rx} ${y+c*ry} ${x-rx} ${y}C${x-rx} ${y-c*ry} ${x-c*rx} ${y-ry} ${x} ${y-ry}C${x+c*rx} ${y-ry} ${x+rx} ${y-c*ry} ${x+rx} ${y}Z`);}
   if(!commands.length)return;for(const command of commands)command.points=command.points.map(([x,y])=>[(m[0]*x+m[2]*y+m[4]-v[0])/v[2],(m[1]*x+m[3]*y+m[5]-v[1])/v[3]]);
   if(commands.some(c=>c.points.flat().some(n=>!Number.isFinite(n)||n<0||n>1)))throw Error('SVG fuori viewBox:espandi il viewBox o ritaglia nel tuo editor.');
   shapes.push({name:(el.id||tag).slice(0,100),color:color.toUpperCase(),opacity:opacity*alpha,svg:{commands}});if(shapes.length>20)throw Error('SVG oltre20 forme:semplifica prima dell’importazione.');
  }
  walk(root,identity,{},1,0);if(!shapes.length)throw Error('SVG senza forme visibili supportate.');return{shapes,ratio:v[2]/v[3]};
 }
 function configure(host){const box=document.createElement('details');box.id='motion-svg-import';box.innerHTML=`<summary>Importa vettore SVG editabile</summary><p class="muted">Colori pieni, curve Bézier, forme e gruppi trasformati. Testo convertito in tracciati: resta vettore, non diventa testo riscrivibile. Espandi tratti ed effetti nell’editor SVG. Script, link, immagini incorporate, filtri e gradienti vengono rifiutati, mai rimossi silenziosamente.</p><label>File SVG (max2MiB,20forme)<input id="motion-svg-file" type="file" accept=".svg,image/svg+xml"></label><p id="motion-svg-status" role="status"></p>`;document.querySelector('#motion-layers').after(box);
  document.querySelector('#motion-svg-file').onchange=async e=>{try{const f=e.target.files[0];if(!f)return;if(f.size>2*1024*1024)throw Error('SVG oltre2MiB.');const parsed=parse(await f.text());await host.add(parsed,f.name);document.querySelector('#motion-svg-status').textContent=parsed.shapes.length+' forme vettoriali importate. Modifica colori, geometria e keyframe; salva e prepara l’anteprima.';}catch(err){host.error(err);}finally{e.target.value='';}};
  document.querySelector('#motion-inspector').addEventListener('change',e=>{if(!e.target.matches('[data-motion-svg]'))return;try{const commands=JSON.parse(e.target.value),l=host.layer();if(l?.kind!=='svg')return;host.before();l.svg={commands};host.changed();}catch(err){host.error(err);}});
 }
 function inspector(l){return l.kind==='svg'?`<details><summary>Curve SVG · coordinate editabili</summary><p class="muted">Comandi numerici M/L/C/Z normalizzati al viewBox; colore, ritaglio e trasformazioni restano nativi. Il reveal del tratto è riservato ai tracciati a vertici.</p><label>Geometria JSON<textarea data-motion-svg rows="6">${JSON.stringify(l.svg.commands)}</textarea></label></details>`:'';}
 return{parse,path,configure,inspector};
})();
