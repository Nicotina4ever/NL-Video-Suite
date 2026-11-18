'use strict';
// Exercise the same two visible clicks as a user, not a hidden showTab call.
const groups={projects:'materials',studio:'edit',longform:'edit',subtitles:'edit',motion:'edit',graphics:'edit',pipeline:'edit',shorts:'social',social:'social',story:'story',workflows:'story',jobs:'tools',system:'tools',storage:'tools'};
async function navigate(page,tab){
 const dialog=page.locator('#export-dialog[open]');if(await dialog.count())await page.locator('#export-close').click();
 const target=page.locator(`[data-tab="${tab}"]`);
 if(!await target.isVisible())await page.locator(`button[data-workflow="${groups[tab]}"]`).click();
 if(!await target.isVisible()&&['subtitles','graphics','pipeline'].includes(tab))await page.locator('#workflow-advanced > summary').click();
 await target.click();
 // Existing Motion tests exercise the optional manual tools. Open their visible
 // disclosure explicitly; the free-idea test enters through the primary task.
 if(tab==='motion'){
  const manual=page.locator('#motion-manual-presets');
  if(await manual.count()&&!await manual.evaluate(e=>e.open))await manual.locator('summary').click();
 }
}
module.exports={navigate};
