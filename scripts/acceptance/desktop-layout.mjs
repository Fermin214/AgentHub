export async function layout(page, evidence) {
 const assert=(value,message)=>{if(!value)throw Error(message);};

 const snapshot=await page.evaluate(()=>window.__TAURI_INTERNALS__.invoke('dispatch',{method:'snapshot',args:{}}));

 const results=[];
 for (const lang of ['zh','en']) {
  await page.locator('.nav-item').last().click();
  await page.getByRole('tab',{name:/^(关于|About)$/}).click();
  await page.getByRole('combobox',{name:/^(界面语言|Language)$/}).selectOption(lang);
  await page.locator('.nav-item').first().click();
  await page.locator('.prompt-card').first().waitFor();
  const viewport=await page.evaluate(()=>[innerWidth,innerHeight]);
  const cards=await page.locator('.prompt-card').evaluateAll(nodes=>nodes.map(n=>{
   const h=n.querySelector('h3'),a=n.querySelector('.prompt-card__actions');
   return {height:n.getBoundingClientRect().height,title:h.getBoundingClientRect().height,line:parseFloat(getComputedStyle(h).lineHeight),overlap:h.getBoundingClientRect().right>a.getBoundingClientRect().left,overflow:n.scrollWidth>n.clientWidth};
  }));
  assert(cards.length===4 && cards.every(c=>c.height===220&&c.title<=c.line*2+1&&!c.overlap&&!c.overflow),'Card bounds');
  const title=page.locator('.prompt-card__title').filter({hasText:'UnbrokenLongTitleForWrapping'});
  await title.focus(); await page.keyboard.press('Enter');
  const dialog=page.getByRole('dialog'); await dialog.waitFor();
  assert((await dialog.locator('h2').innerText()).length>250,'Incomplete preview title');
  assert((await dialog.locator('.prompt-preview-body').innerText()).includes('Second line'),'Missing preview body');
  const box=await dialog.evaluate(n=>({overflow:n.scrollWidth>n.clientWidth,body:n.querySelector('.modal__body').clientHeight,bottom:n.getBoundingClientRect().bottom}));
  assert(!box.overflow&&box.body>60&&box.bottom<=innerHeightFor(viewport),'Preview bounds');
  await page.screenshot({path:`${evidence}/native-preview-${lang}-${viewport[0]}.png`});
  await page.keyboard.press('Escape'); await dialog.waitFor({state:'hidden'});
  assert(await title.evaluate(n=>n===document.activeElement),'Focus return');
  await page.locator('.nav-item').nth(1).click();
  await page.getByRole('button',{name:'acceptance-writer',exact:true}).waitFor();
  const actions=await page.locator('.skill-row .library-actions').evaluateAll(nodes=>nodes.map(n=>{
   const r=[...n.querySelectorAll('button')].map(b=>b.getBoundingClientRect());
   return {spread:Math.max(...r.map(b=>b.top+b.height/2))-Math.min(...r.map(b=>b.top+b.height/2)),right:r.at(-1).right,bound:n.closest('article').getBoundingClientRect().right};
  }));
  assert(actions.every(a=>a.spread<=1&&a.right<=a.bound+1),'Wrapped Skill actions');
  const typography=await page.locator('.skill-check-summary').first().evaluate(n=>[...n.children].map(c=>{const s=getComputedStyle(c);return {family:s.fontFamily,size:s.fontSize,weight:s.fontWeight,line:s.lineHeight};}));
  assert(typography.length===2&&JSON.stringify(typography[0])===JSON.stringify(typography[1]),'AH-011 mismatch');
  await page.screenshot({path:`${evidence}/native-skills-${lang}-${viewport[0]}.png`});
  await page.getByRole('button',{name:'acceptance-writer',exact:true}).click();
  const body=dialog.locator('.toolkit-reader__content');
  await body.getByText('LIBRARY_COPY',{exact:false}).waitFor();
  assert(!(await body.innerText()).includes('INSTALLED_COPY'),'Read installed copy');
  assert(await dialog.locator('select').count()===0,'Location selector remains');
  const nav=dialog.locator('.toolkit-reader nav');
  const before=await nav.evaluate(n=>n.scrollTop);
  await body.focus(); await body.press('PageDown');
  await page.waitForFunction(()=>document.querySelector('.toolkit-reader__content').scrollTop>0);
  assert(await nav.evaluate(n=>n.scrollTop)===before,'File list scrolled with body');
  await nav.getByRole('button',{name:'references/note-1.md',exact:false}).click();
  await body.getByText('Fictional reference content.',{exact:false}).waitFor();
  await page.screenshot({path:`${evidence}/native-detail-${lang}-${viewport[0]}.png`});
  await page.keyboard.press('Escape');
  results.push({lang,viewport,cards,actions,typography,libraryOnly:true,keyboardScroll:true,fileSwitch:true});
 }
 return results;
 function innerHeightFor(v){return v[1];}
}
