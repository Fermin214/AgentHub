export async function headerReview(page, evidence) {
 const results=[];
 const assert=(ok,message)=>{if(!ok)throw Error(message);};
 await page.waitForFunction(()=>innerWidth===860&&innerHeight===640);
 for(const lang of ['zh','en']){
  await page.locator('.nav-item').last().click();
  await page.getByRole('tab',{name:/^(关于|About)$/}).click();
  await page.getByRole('combobox',{name:/^(界面语言|Language)$/}).selectOption(lang);
  await page.locator('.nav-item').nth(1).click();
  await page.getByRole('button',{name:'acceptance-second',exact:true}).click();
  const dialog=page.getByRole('dialog');
  await dialog.getByText('LIBRARY_COPY',{exact:false}).waitFor();
  const link=dialog.locator('.modal__title-row a');
  assert(await link.getAttribute('href')==='https://github.com/agenthub-fixtures/layout-only','Incorrect remote source href');
  assert((await link.innerText()).includes(lang==='zh'?'打开来源仓库':'Open source repository'),'Source link language mismatch');
  const geometry=await dialog.evaluate(n=>{
   const rect=x=>{const r=x.getBoundingClientRect();return {left:r.left,right:r.right,top:r.top,bottom:r.bottom};};
   return {title:rect(n.querySelector('h2')),link:rect(n.querySelector('.modal__title-row a')),close:rect(n.querySelector('.modal__header > .icon-button')),overflow:n.scrollWidth>n.clientWidth};
  });
  assert(!geometry.overflow&&geometry.title.right<=geometry.link.left,'Source link overlaps title');
  assert(geometry.link.bottom<=640&&geometry.link.right<=860&&geometry.close.bottom<=640&&geometry.close.right<=860,'Header controls out of bounds');
  await link.focus();assert(await link.evaluate(n=>n===document.activeElement),'Link not keyboard reachable');
  await page.screenshot({animations:'disabled',path:`${evidence}/native-remote-header-${lang}-860.png`});
  await page.keyboard.press('Escape');await dialog.waitFor({state:'hidden'});
  await page.locator('.nav-item').first().click();
  await page.locator('.prompt-card__title').filter({hasText:'UnbrokenLongTitleForWrapping'}).click();
  await page.getByRole('dialog').locator('.prompt-preview-body').waitFor();
  await page.screenshot({animations:'disabled',path:`${evidence}/native-header-prompt-${lang}-860.png`});
  await page.keyboard.press('Escape');
  results.push({lang,viewport:[860,640],geometry,remoteHref:true,keyboardReachable:true,method:'Real packaged WebView2 and backend snapshot; fictional remote metadata, no Git fetch or external launch'});
 }
 return results;
}

export async function layout(page, evidence) {
 const assert=(value,message)=>{if(!value)throw Error(message);};
 // Keyboard scrolling is animated by WebView2. Sample only after the position
 // stays unchanged across frames; an initial nonzero offset is not completion.
 const settledScroll=locator=>locator.evaluate(n=>new Promise((resolve,reject)=>{
  const start=performance.now();let value=n.scrollTop,since=start;
  function sample(now){
   if(n.scrollTop!==value){value=n.scrollTop;since=now;}
   if(now-since>=150){resolve(value);return;}
   if(now-start>3000){reject(Error('Scroll did not settle'));return;}
   requestAnimationFrame(sample);
  }
  requestAnimationFrame(sample);
 }));

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
  await page.screenshot({animations: 'disabled',path:`${evidence}/native-preview-${lang}-${viewport[0]}.png`});
  await page.keyboard.press('Escape'); await dialog.waitFor({state:'hidden'});
  assert(await title.evaluate(n=>n===document.activeElement),'Focus return');
  await page.locator('.nav-item').nth(1).click();
  await page.getByRole('button',{name:'acceptance-writer',exact:true}).waitFor();
  const icons=await page.locator('.skill-row .agent-brand-icon').evaluateAll(nodes=>nodes.map(n=>({loaded:n.complete&&n.naturalWidth>0,filter:getComputedStyle(n).filter})));
  assert(icons.length>0&&icons.every(icon=>icon.loaded&&icon.filter==='none'),'Original Agent icons did not load');
  const actions=await page.locator('.skill-row .library-actions').evaluateAll(nodes=>nodes.map(n=>{
   const r=[...n.querySelectorAll('button')].map(b=>b.getBoundingClientRect());
   return {spread:Math.max(...r.map(b=>b.top+b.height/2))-Math.min(...r.map(b=>b.top+b.height/2)),right:r.at(-1).right,bound:n.closest('article').getBoundingClientRect().right};
  }));
  assert(actions.every(a=>a.spread<=1&&a.right<=a.bound+1),'Wrapped Skill actions');
  const typography=await page.locator('.skill-check-summary').first().evaluate(n=>[...n.children].map(c=>{const s=getComputedStyle(c);return {family:s.fontFamily,size:s.fontSize,weight:s.fontWeight,line:s.lineHeight};}));
  assert(typography.length===2&&JSON.stringify(typography[0])===JSON.stringify(typography[1]),'AH-011 mismatch');
  await page.screenshot({animations: 'disabled',path:`${evidence}/native-skills-${lang}-${viewport[0]}.png`});
  await page.getByRole('button',{name:'acceptance-writer',exact:true}).click();
  const body=dialog.locator('.toolkit-reader__content');
  await body.getByText('LIBRARY_COPY',{exact:false}).waitFor();
  assert(!(await body.innerText()).includes('INSTALLED_COPY'),'Read installed copy');
  assert(await dialog.locator('select').count()===0,'Location selector remains');
  assert(await dialog.locator('header a').count()===0,'Local Skill unexpectedly exposes an external source link');
  const nav=dialog.locator('.toolkit-reader nav');
  const before=await nav.evaluate(n=>n.scrollTop);
  await body.focus(); await body.press('PageDown');
  await page.waitForFunction(()=>document.querySelector('.toolkit-reader__content').scrollTop>0);
  await settledScroll(body);
  assert(await nav.evaluate(n=>n.scrollTop)===before,'File list scrolled with body');
  const contentScroll=await body.evaluate(n=>n.scrollTop);
  await nav.focus();await nav.press('PageDown');
  await page.waitForFunction(()=>document.querySelector('.toolkit-reader nav').scrollTop>0);
  await settledScroll(nav);
  assert(await body.evaluate(n=>n.scrollTop)===contentScroll,'Body scrolled with file list');
  await nav.press('Control+Home');
  await page.waitForFunction(()=>document.querySelector('.toolkit-reader nav').scrollTop===0);
  await settledScroll(nav);
  await body.focus();await body.press('PageUp');
  await page.waitForFunction(previous=>document.querySelector('.toolkit-reader__content').scrollTop<previous,contentScroll);
  await settledScroll(body);
  const wheelBefore=await body.evaluate(n=>n.scrollTop);
  await body.hover();await page.mouse.wheel(0,400);
  await page.waitForFunction(previous=>document.querySelector('.toolkit-reader__content').scrollTop>previous,wheelBefore);
  await settledScroll(body);
  assert(await nav.evaluate(n=>n.scrollTop)===0,'Mouse scroll affected both panes');
  const close=dialog.getByRole('button',{name:/^(关闭|Close)$/}).first();
  assert(await close.evaluate(n=>{const r=n.getBoundingClientRect();return r.top>=0&&r.bottom<=innerHeight&&r.left>=0&&r.right<=innerWidth;}),'Viewer close control out of bounds');
  await nav.getByRole('button',{name:'references/note-1.md',exact:false}).click();
  await body.getByText('Fictional reference content.',{exact:false}).waitFor();
  await page.screenshot({animations: 'disabled',path:`${evidence}/native-detail-${lang}-${viewport[0]}.png`});
  await page.keyboard.press('Escape');
  results.push({lang,viewport,cards,actions,typography,icons,libraryOnly:true,keyboardScroll:true,independentMouseScroll:true,pageUp:true,localSourceLinkAbsent:true,fileSwitch:true});
 }
 return results;
 function innerHeightFor(v){return v[1];}
}
