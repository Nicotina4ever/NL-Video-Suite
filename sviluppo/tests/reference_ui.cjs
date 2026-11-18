const {navigate}=require('./navigation.cjs');
// Read-only check of the running suite with the authentic reference project.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const path=require('node:path');
async function main(){
  const base='http://127.0.0.1:8088';
  const projects=await(await fetch(base+'/api/projects')).json();
  const reference=projects.find(p=>p.id==='cdb08b2d-0b85-4554-aa5e-d6ca0584dbbc');
  assert(reference && reference.assets.length===7 && reference.timeline.clips.length===0);
  const browser=await chromium.launch({channel:'msedge',headless:true});
  try{
    const page=await browser.newPage({viewport:{width:1550,height:1000}});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.goto(base);
    await page.locator('#projects').selectOption(reference.id);
    await page.waitForFunction(()=>document.querySelectorAll('#assets .asset').length===7);
    await page.locator('#assets .asset').filter({hasText:'Nicola_Profile_1'}).locator('[data-preview]').click();
    await page.waitForFunction(()=>document.querySelector('#image-preview').complete && document.querySelector('#image-preview').naturalWidth>0);
    await page.screenshot({path:path.join(__dirname,'artifacts','riferimenti-studio.png'),fullPage:true});
    await navigate(page,'graphics');
    assert((await page.locator('#creative-brief').inputValue()).includes('#7531FF'));
    await navigate(page,'pipeline');
    await page.locator('details.flow-legacy summary').first().click();
    assert((await page.locator('#transcripts').innerText()).includes('479 parole'));
    await navigate(page,'system');
    await page.waitForFunction(()=>document.querySelector('#comfy-url').value==='http://127.0.0.1:8188');
    await page.screenshot({path:path.join(__dirname,'artifacts','sistema-reale.png'),fullPage:true});
    assert.deepEqual(errors,[]);
    console.log('PASS real UI: seven reference copies, portrait preview, saved brief, 479-word transcript, operational System; no project writes');
  }finally{await browser.close();}
}
main().catch(e=>{console.error(e);process.exitCode=1;});
