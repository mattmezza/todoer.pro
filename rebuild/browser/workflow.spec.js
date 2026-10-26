import {test,expect} from '@playwright/test';
const auth = {Authorization:'Bearer local-integration-token-not-for-production'};
async function signin(page){await page.goto('/signin');await page.getByRole('button',{name:'Enter development workspace'}).click();await expect(page.locator('h1')).toHaveText('#Groceries');}
test('capture, optimistic completion, edit conflict and draft retention',async({page,request})=>{
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await signin(page);
  const title=`Browser task ${crypto.randomUUID()}`;
  await page.locator('#capture-title').fill(title);await page.locator('#capture-form button').click();
  const row=page.locator('.task').filter({has:page.getByRole('button',{name:title,exact:true})});
  await expect(row).toHaveCount(1);
  await row.locator('.completion').click();await expect(row).toHaveAttribute('data-complete','true');
  await row.locator('.completion').click();await expect(row).toHaveAttribute('data-complete','false');
  await row.locator('.task-title').click();await page.locator('#edit-title').fill('Human unsaved draft');
  const id=await row.getAttribute('data-id');
  const before=await request.get(`/api/v1/tasks/${id}/subtree`,{headers:auth});const [task]=await before.json();
  const update=await request.patch(`/api/v1/tasks/${id}`,{headers:auth,data:{revision:task.revision,title:'Agent concurrent edit'}});expect(update.status()).toBe(200);
  await page.waitForTimeout(2500);await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  await page.locator('#edit-form button[type=submit]').click();await expect(page.locator('#edit-error')).toContainText('stale revision');
  await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  await page.reload();await expect(page.locator('#task-editor')).toBeVisible();await expect(page.locator('#edit-title')).toHaveValue('Human unsaved draft');
  expect(errors).toEqual([]);
});
for(const [name,width,height] of [['phone',390,844],['tablet',820,1180],['desktop',1440,1000]]){
  test(`${name}: navigation, touch targets and no overflow`,async({page})=>{
    await page.setViewportSize({width,height});await signin(page);await page.locator('#capture-title').blur();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
    const completion=page.locator('.completion').first();const box=await completion.boundingBox();expect(Math.round(box.width)).toBeGreaterThanOrEqual(44);expect(Math.round(box.height)).toBeGreaterThanOrEqual(44);
    if(width<640){await page.getByRole('button',{name:'Toggle navigation'}).click();await expect(page.locator('#navigation')).toBeVisible();await page.locator('#navigation').getByRole('link',{name:'Inbox',exact:true}).first().click();await expect(page.locator('h1')).toHaveText('#Inbox');}
    await page.screenshot({path:`.local/rebuild/rust-${name}-${test.info().project.name}.png`,fullPage:true});
  });
}
test('light theme and reduced motion remain usable',async({page})=>{await page.emulateMedia({colorScheme:'light',reducedMotion:'reduce'});await signin(page);expect(await page.evaluate(()=>getComputedStyle(document.body).backgroundColor)).toBe('rgb(255, 255, 255)');await page.screenshot({path:`.local/rebuild/rust-light-${test.info().project.name}.png`,fullPage:true});});
