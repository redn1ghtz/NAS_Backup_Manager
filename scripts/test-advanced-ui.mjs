// Real isolated WebView/IPC test. No user state, scheduler or real service accounts.
import assert from 'node:assert/strict';
import {DatabaseSync} from 'node:sqlite';
import {spawn} from 'node:child_process';
import {copyFileSync,mkdirSync,mkdtempSync,writeFileSync,readFileSync,unlinkSync} from 'node:fs';
import {join,resolve,basename} from 'node:path';
import {createServer} from 'node:net';
const root=resolve(import.meta.dirname,'..');
const packageFolder=process.env.NAS_BACKUP_TEST_PACKAGE??join(root,'portable/NAS Backup Manager Preview');
const fixture=mkdtempSync(join(root,'target/advanced-ui-'));
for(const name of ['NAS Backup Manager.exe','nas-backup.exe','nas-backup-notify.exe'])copyFileSync(join(packageFolder,name),join(fixture,name));
writeFileSync(join(fixture,'portable.flag'),'Disposable UI test');
mkdirSync(join(fixture,'tools'));copyFileSync(join(packageFolder,'tools/rclone.exe'),join(fixture,'tools/rclone.exe'));
const source=join(fixture,'source'),destination=join(fixture,'destination');mkdirSync(source);mkdirSync(destination);
writeFileSync(join(source,'fixture.txt'),'Disposable UI fixture');
for(let i=0;i<5;i++)writeFileSync(join(source,`small-${i}.txt`),'Disposable resize fixture');
const server=createServer();await new Promise(r=>server.listen(0,'127.0.0.1',r));const port=server.address().port;await new Promise(r=>server.close(r));
const app=spawn(join(fixture,'NAS Backup Manager.exe'),[],{cwd:fixture,windowsHide:true,stdio:'ignore',env:{...process.env,WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS:`--remote-debugging-port=${port}`}});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn){for(let i=0;i<200;i++){const v=await fn();if(v)return v;await sleep(100);}throw new Error('Timed out');}
let socket;
try{
 const page=await until(async()=>{try{return(await(await fetch(`http://127.0.0.1:${port}/json`)).json()).find(p=>p.type==='page'&&p.url.includes('tauri.localhost'));}catch{return false;}});
 socket=new WebSocket(page.webSocketDebuggerUrl);await new Promise((r,j)=>{socket.onopen=r;socket.onerror=j;});
 let sequence=0;const pending=new Map();socket.onmessage=e=>{const d=JSON.parse(e.data);if(pending.has(d.id)){pending.get(d.id)(d);pending.delete(d.id);}};
 async function cdp(method,params={}){const id=++sequence;const result=new Promise(r=>pending.set(id,r));socket.send(JSON.stringify({id,method,params}));const response=await result;assert(!response.error,JSON.stringify(response));return response.result;}
 async function evaluate(expression){const result=await cdp('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});assert(!result.exceptionDetails,JSON.stringify(result));return result.result.value;}
 const invoke=(command,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`);
 const backup=request=>invoke('backup_command',{request});
 await until(()=>evaluate('Boolean(window.__TAURI_INTERNALS__?.invoke)'));
 await invoke('set_notifications',{enabled:false});
 const generatedPath=join(fixture,'generated-recovery.agekey');
 const generated=await backup({action:'generateIdentity',ageExecutable:join(root,'target/optional-tools/x64/age.exe'),path:generatedPath});
 assert(generated.recipient.startsWith('age1'));
 assert.equal(readFileSync(generatedPath,'utf8'),generated.identity);
 let overwriteRejected=false;
 try{await backup({action:'generateIdentity',ageExecutable:join(root,'target/optional-tools/x64/age.exe'),path:generatedPath});}catch{overwriteRejected=true;}
 assert(overwriteRejected);
 assert.equal(readFileSync(generatedPath,'utf8'),generated.identity);

 const connection=await backup({action:'prepareCloud',provider:'onedrive'});
 assert.equal(connection.connected,false);assert(readFileSync(connection.config,'utf8').includes('RCLONE_ENCRYPT_V0:'));
 assert.equal(Object.keys(connection).some(key=>key.toLowerCase().includes('password')),false);
 await backup({action:'cancelCloudAuthorization',id:connection.connectionId});
 let cancelled=false;try{await backup({action:'authorizeCloud',id:connection.connectionId});}catch{cancelled=true;}assert(cancelled);
 await until(()=>evaluate('Array.from(document.querySelectorAll("button")).some(b=>b.textContent.includes("Новая задача"))'));

 await cdp('Emulation.setDeviceMetricsOverride',{width:1024,height:700,deviceScaleFactor:1,mobile:false});
 const click=text=>evaluate(`(()=>{const b=Array.from(document.querySelectorAll('button')).find(b=>b.textContent.trim()===${JSON.stringify(text)});if(!b)throw Error('Missing button');b.click();return true;})()`);
 const input=(id,value)=>evaluate(`(()=>{const e=document.getElementById(${JSON.stringify(id)});if(!e)throw Error('Missing input');const setter=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(e),'value').set;setter.call(e,${JSON.stringify(value)});e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}));return true;})()`);
 const summary=text=>evaluate(`(()=>{const e=Array.from(document.querySelectorAll('summary')).find(e=>e.textContent.trim()===${JSON.stringify(text)});e.parentElement.open=true;return true;})()`);
 await click('Новая задача');await until(()=>evaluate('Boolean(document.getElementById("job-name"))'));
 await input('job-name','Advanced UI fixture');await input('source',source);await click('Далее');await until(()=>evaluate('Boolean(document.getElementById("destination"))'));await input('destination',destination);await input('destination-kind','cloud');await click('Далее');
 await until(()=>evaluate('Boolean(document.getElementById("cloud-provider"))'));
 assert.equal(await evaluate('Boolean(document.querySelector(".advanced-settings"))'),false);
 assert.equal(await evaluate('Array.from(document.querySelectorAll("button")).find(b=>b.textContent.trim()==="Далее").disabled'),true);
 assert.equal(await evaluate('Boolean(document.getElementById("advanced-rcloneConfig"))'),false);
 assert.deepEqual(await evaluate('Array.from(document.getElementById("cloud-provider").options).map(o=>o.value).sort()'),['dropbox','onedrive']);
 const cloudImage=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/cloud-setup-qa.png'),Buffer.from(cloudImage.data,'base64'));
 await click('Назад');await input('destination-kind','local');await click('Далее');
 await until(()=>evaluate('Boolean(document.querySelector(".advanced-settings"))'));
 await summary('Дополнительные возможности');await summary('Telegram и почта');
 const notificationInputs=await evaluate(`(()=>{const section=Array.from(document.querySelectorAll('.advanced-settings details')).find(d=>d.querySelector('summary')?.textContent==='Telegram и почта');return Array.from(section.querySelectorAll('input')).map(e=>e.type);})()`);assert.deepEqual(notificationInputs,['checkbox','checkbox']);
 await summary('Целостность · SHA-256');
 await evaluate(`document.querySelector('.advanced-settings input[type="checkbox"]').click()`);
 await input('advanced-verifyEveryDays','14');await input('advanced-verifyMibPerSecond','0');
 await summary('Повторы при временных сбоях');await input('advanced-retryAttempts','5');
 await summary('Скорость по времени суток');await input('advanced-bandwidthKib','128');await input('bandwidthStartMinute','22:00');await input('bandwidthEndMinute','07:00');
 await evaluate(`document.querySelector('.advanced-settings').scrollIntoView({block:'start'})`);
 const screenshot=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/advanced-form-qa.png'),Buffer.from(screenshot.data,'base64'));
 await click('Далее');await click('Сохранить и проверить');
 let jobs=await until(async()=>{const jobs=await backup({action:'listJobs',offset:0});return jobs.length?jobs:false;});
 assert.equal(jobs[0].advanced.verifyEnabled,true);assert.equal(jobs[0].advanced.verifyEveryDays,14);assert.equal(jobs[0].advanced.retryAttempts,5);assert.equal(jobs[0].advanced.bandwidthKib,128);assert.equal(jobs[0].advanced.bandwidthStartMinute,1320);assert.equal(jobs[0].advanced.bandwidthEndMinute,420);
 await until(()=>evaluate('Boolean(document.querySelector(".preview-dialog"))'));await click('Закрыть');
 assert.equal(await evaluate(`document.querySelector('.cloud-capacity')===null`),true);
 // Edit through the same real form; no cloud settings or fake statuses are injected.
 await evaluate(`document.querySelector('button[aria-label^="Действия"]').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,button:0,pointerType:'mouse'}))`);
 await until(()=>evaluate(`Array.from(document.querySelectorAll('[role="menuitem"]')).some(e=>e.textContent.includes('Изменить'))`));
 await evaluate(`Array.from(document.querySelectorAll('[role="menuitem"]')).find(e=>e.textContent.includes('Изменить')).click()`);
 await until(()=>evaluate('Boolean(document.getElementById("job-name"))'));await click('Далее');await click('Далее');
 await until(()=>evaluate('Boolean(document.querySelector(".advanced-settings"))'));await summary('Дополнительные возможности');await summary('Целостность · SHA-256');assert.equal(await evaluate('document.getElementById("advanced-verifyEveryDays").value'),'14');await input('advanced-verifyEveryDays','21');await click('Далее');await click('Сохранить и проверить');
 jobs=await until(async()=>{const jobs=await backup({action:'listJobs',offset:0});return jobs[0]?.advanced.verifyEveryDays===21?jobs:false;});
 assert.equal(jobs[0].advanced.retryAttempts,5);assert.equal(jobs[0].advanced.cloudMode,'off');assert.equal(jobs[0].advanced.verifyMibPerSecond,0);
 await until(()=>evaluate('Boolean(document.querySelector(".preview-dialog"))'));await click('Закрыть');
 writeFileSync(join(source,'large.bin'),Buffer.alloc(4*1024*1024,42));
 const verifiedJob=await backup({action:'saveJob',draft:{...jobs[0],advanced:{...jobs[0].advanced,verifyEnabled:false,verifyMibPerSecond:1}}});
 const runPlan=await backup({action:'preview',id:verifiedJob.id});await backup({action:'run',id:verifiedJob.id,previewId:runPlan.id,consent:true});
 await evaluate(`window.__integrityRequest=window.__TAURI_INTERNALS__.invoke('backup_command',{request:{action:'verifyIntegrity',id:${JSON.stringify(verifiedJob.id)}}});true`);
 await until(()=>evaluate('document.querySelector(".sync-progress")?.textContent.includes("Обработано файлов")'));
 await until(async()=>{const active=await backup({action:'activity'});return active[0]?.verifiedBytes>0&&active[0]?.verifiedBytes<active[0]?.totalBytes;});
 const progressImage=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/integrity-progress-qa.png'),Buffer.from(progressImage.data,'base64'));
 await evaluate('window.__integrityRequest');jobs=await backup({action:'listJobs',offset:0});
 const cloudDraft={...jobs[0],advanced:{...jobs[0].advanced,cloudMode:'replica',cloudProvider:'dropbox',cloudRemote:`fixture:ui-cloud-${basename(fixture)}`,bandwidthKib:512,bandwidthStartMinute:0,bandwidthEndMinute:0,rcloneExecutable:join(root,'target/optional-tools/x64/rclone.exe'),rcloneConfig:join(root,'target/optional-tools/fixture-rclone.conf')},secrets:{rclonePassword:'fixture-only-not-a-real-password'}};
 const cloudJob=await backup({action:'saveJob',draft:cloudDraft});
 await until(()=>evaluate('Boolean(document.querySelector(".cloud-capacity"))'));
 assert.equal(JSON.stringify(cloudJob).includes('fixture-only-not-a-real-password'),false);
 assert((await evaluate('document.querySelector(".cloud-capacity h3").textContent')).includes('Dropbox'));
 await until(()=>evaluate('document.querySelectorAll(".storage-panel .storage-ring").length===2'));
 assert(await evaluate('document.querySelector(".cloud-capacity").compareDocumentPosition(document.querySelector(".local-capacity")) & Node.DOCUMENT_POSITION_FOLLOWING'));
 await until(()=>evaluate('!document.querySelector(".cloud-capacity button").disabled'));
 const cacheDb=new DatabaseSync(join(fixture,'data/state.sqlite'));
 try{cacheDb.prepare('UPDATE cloud_capacity SET total=1000,used=250,free=750,queried_at=? WHERE job_id=?').run(Math.floor(Date.now()/1000)+3600,cloudJob.id);}finally{cacheDb.close();}
 assert.equal((await backup({action:'cloudCapacity',id:cloudJob.id,force:false})).total,1000);
 await evaluate('document.querySelector(".cloud-capacity button").click()');
 await until(async()=>(await backup({action:'cloudCapacity',id:cloudJob.id,force:false})).total!==1000);
 await until(()=>evaluate('!document.querySelector(".cloud-capacity button").disabled'));
 // Transfer through real rclone to a disposable local alias; no real account.
 const cloudPlan=await backup({action:'preview',id:cloudJob.id});
 await evaluate(`window.__cloudRun=window.__TAURI_INTERNALS__.invoke('backup_command',{request:{action:'run',id:${JSON.stringify(cloudJob.id)},previewId:${JSON.stringify(cloudPlan.id)},consent:true}});true`);
 const live=await until(async()=>{const a=await backup({action:'activity'});return a.find(p=>p.phase==='cloud'&&p.copiedBytes>0&&p.copiedBytes<p.totalBytes);});
 assert(live.totalBytes>=4*1024*1024);
 await until(()=>evaluate('document.querySelector(".sync-progress progress")?.value>0'));
 const activeLayouts=[];
 for(const width of [1024,1400]){await cdp('Emulation.setDeviceMetricsOverride',{width,height:700,deviceScaleFactor:1,mobile:false});await sleep(100);const layout=await evaluate('({width:innerWidth,height:innerHeight,page:document.documentElement.scrollHeight})');activeLayouts.push(layout);assert(layout.page<=layout.height+1,JSON.stringify(layout));}
 await cdp('Page.reload');
 await until(async()=>{try{return await evaluate('Boolean(window.__TAURI_INTERNALS__?.invoke)');}catch{return false;}});
 await until(()=>evaluate('Boolean(document.querySelector(".sync-progress"))'));
 await until(()=>evaluate('document.querySelector(".sync-progress progress")?.value>0'));
 const progressShot=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/cloud-transfer-progress-qa.png'),Buffer.from(progressShot.data,'base64'));
 await until(async()=>(await backup({action:'listJobs',offset:0}))[0]?.lastResult==='succeeded'&&(await backup({action:'activity'})).length===0);
 await until(()=>evaluate('document.querySelectorAll(".storage-panel .storage-ring").length===2'));
 await until(()=>evaluate('!document.querySelector(".sync-progress")'));
 // One task, quota enabled, recent operations: every main panel fits without page scroll.
 const layouts=[];
 for(const width of [1024,1400]){
  await cdp('Emulation.setDeviceMetricsOverride',{width,height:700,deviceScaleFactor:1,mobile:false});
  await evaluate('window.scrollTo(0,0)');await sleep(100);
  const layout=await evaluate('({width:innerWidth,height:innerHeight,page:document.documentElement.scrollHeight,panels:Array.from(document.querySelectorAll(".dashboard-layout>.panel,.dashboard-layout .jobs-panel,.dashboard-layout .storage-panel,.dashboard-utilities>.panel")).map(p=>({bottom:p.getBoundingClientRect().bottom,right:p.getBoundingClientRect().right}))})');
  if(layout.page>layout.height+1){const shot=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/compact-layout-debug.png'),Buffer.from(shot.data,'base64'));console.log(await evaluate('Array.from(document.querySelector("main").children).map(p=>({tag:p.className,top:p.getBoundingClientRect().top,height:p.getBoundingClientRect().height,text:p.textContent.slice(0,110)}))'));}
  layouts.push(layout);assert(layout.page<=layout.height+1,JSON.stringify(layout));assert(layout.panels.every(p=>p.right<=layout.width+1&&p.bottom<=layout.height+1),JSON.stringify(layout));
 }
 await cdp('Emulation.setDeviceMetricsOverride',{width:1400,height:700,deviceScaleFactor:1,mobile:false});
 await evaluate('document.querySelector(".storage-panel").scrollIntoView({block:"start"})');
 const rows=await evaluate('Array.from(document.querySelectorAll(".cloud-capacity .storage-details dl>div")).map(row=>{const a=row.querySelector("dt").getBoundingClientRect(),b=row.querySelector("dd").getBoundingClientRect();return {separate:a.right<=b.left+1||b.top>=a.bottom-1};})');assert.equal(rows.length,3);assert(rows.every(row=>row.separate));
 const storageImage=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/cloud-storage-qa.png'),Buffer.from(storageImage.data,'base64'));

 // Actual plan operations must carry the requested colors.
 writeFileSync(join(source,'fixture.txt'),'Changed disposable fixture');
 writeFileSync(join(source,'brand-new.txt'),'New disposable fixture');unlinkSync(join(source,'small-0.txt'));
 await click('Предпросмотр');await until(()=>evaluate('Boolean(document.querySelector(".preview-dialog"))'));
 const tones=await evaluate('Array.from(document.querySelectorAll(".preview-dialog .operation-label")).map(p=>({tone:p.dataset.tone,color:getComputedStyle(p).color}))');
 for(const [tone,color] of [['delete','rgb(248, 113, 113)'],['update','rgb(250, 204, 21)'],['create','rgb(74, 222, 128)']])assert(tones.some(p=>p.tone===tone&&p.color===color),JSON.stringify(tones));
 await click('Закрыть');
 // More rows are real fixture jobs, with independent disposable destinations.
 for(let i=0;i<14;i++){const dest=join(fixture,`resize-dest-${i}`);mkdirSync(dest);await backup({action:'saveJob',draft:{...cloudJob,id:null,revision:null,name:`Resize fixture ${i}`,destination:dest,advanced:{...cloudJob.advanced,cloudMode:'off',cloudProvider:'',cloudConnection:'',rcloneConfig:'',vaultId:''},secrets:{}}});}
 await until(()=>evaluate('document.querySelectorAll(".jobs-panel .job-row").length>=15'));
 const resizeLayouts=[];
 for(const [width,height] of [[1024,600],[1024,700],[1400,900],[1920,1080]]){
  await cdp('Emulation.setDeviceMetricsOverride',{width,height,deviceScaleFactor:1,mobile:false});await sleep(150);
  const layout=await evaluate('(()=>{const list=document.querySelector(".jobs-panel .job-list"),recent=document.querySelector(".recent-panel .table-scroll");return {width:innerWidth,height:innerHeight,page:document.documentElement.scrollHeight,pageWidth:document.documentElement.scrollWidth,jobsHeight:list.clientHeight,recentHeight:recent.clientHeight,jobsOverflow:list.scrollHeight>list.clientHeight,visibleJobs:Array.from(list.children).filter(p=>{const r=p.getBoundingClientRect(),a=list.getBoundingClientRect();return r.top>=a.top&&r.bottom<=a.bottom;}).length,panels:Array.from(document.querySelectorAll(".dashboard-layout .panel")).map(p=>({bottom:p.getBoundingClientRect().bottom,right:p.getBoundingClientRect().right}))};})()');
  resizeLayouts.push(layout);assert(layout.page<=height+1&&layout.pageWidth<=width+1,JSON.stringify(layout));assert(layout.jobsOverflow);assert(layout.jobsHeight>=80,JSON.stringify(layout));assert(layout.panels.every(p=>p.bottom<=height+1&&p.right<=width+1),JSON.stringify(layout));
 }
 assert(resizeLayouts.at(-1).jobsHeight>resizeLayouts[0].jobsHeight+100,JSON.stringify(resizeLayouts));
 assert(resizeLayouts.at(-1).recentHeight>resizeLayouts[0].recentHeight+100,JSON.stringify(resizeLayouts));
 assert(resizeLayouts.at(-1).visibleJobs>resizeLayouts[0].visibleJobs,JSON.stringify(resizeLayouts));
 const resizeImage=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/responsive-home-qa.png'),Buffer.from(resizeImage.data,'base64'));
 const oneDriveJob=await backup({action:'saveJob',draft:{...cloudJob,advanced:{...cloudJob.advanced,cloudProvider:'onedrive'}}});
 await until(()=>evaluate('document.querySelector(".cloud-capacity h3")?.textContent.includes("OneDrive")'));
 await backup({action:'saveJob',draft:{...oneDriveJob,advanced:{...oneDriveJob.advanced,cloudMode:'off'}}});
 await until(()=>evaluate('document.querySelector(".cloud-capacity")===null'));
 const appStatus=await invoke('get_app_status');assert.equal(appStatus.version,'0.2.0-alpha');assert.equal(appStatus.schemaVersion,13);
 await evaluate(`Array.from(document.querySelectorAll('nav button')).find(b=>b.textContent==='Настройки').click()`);
 await until(()=>evaluate('Boolean(document.getElementById("notification-telegramChat"))&&!document.getElementById("notification-telegramChat").disabled'));
 await summary('Telegram');await summary('Почта · SMTP');
 await input('notification-telegramChat','12345');await input('notification-telegramToken','123:native_fixture_token');
 await input('notification-smtpUrl','smtps://smtp.example.invalid:465');await input('notification-smtpUsername','fixture@example.invalid');await input('notification-smtpFrom','fixture@example.invalid');await input('notification-smtpTo','recipient@example.invalid');await input('notification-smtpPassword','native-fixture-password');
 await click('Сохранить настройки отчётов');
 await until(()=>evaluate('document.querySelector(".global-notification-settings .success")?.textContent.includes("сохранены")'));
 const shared=await backup({action:'notificationSettings'});assert(shared.telegramConfigured&&shared.smtpConfigured);assert(!JSON.stringify(shared).includes('native_fixture_token'));assert(!JSON.stringify(shared).includes('native-fixture-password'));
 assert.equal(await evaluate('document.getElementById("notification-telegramToken").value'), '');assert.equal(await evaluate('document.getElementById("notification-smtpPassword").value'),'');
 const currentJob=(await backup({action:'listJobs',offset:0})).find(j=>j.id===cloudJob.id);
 const inherited=await backup({action:'saveJob',draft:{...currentJob,advanced:{...currentJob.advanced,telegramEnabled:true,smtpEnabled:true,telegramChat:'',smtpUrl:'',smtpUsername:'',smtpFrom:'',smtpTo:''},secrets:{}}});
 assert(inherited.advanced.telegramEnabled&&inherited.advanced.smtpEnabled);
 await evaluate(`Array.from(document.querySelectorAll('nav button')).find(b=>b.textContent==='Главная').click()`);
 await evaluate(`Array.from(document.querySelectorAll('nav button')).find(b=>b.textContent==='Настройки').click()`);
 await until(()=>evaluate('document.querySelector(".global-notification-settings summary")?.textContent.includes("Настроено")'));
 assert.equal(await evaluate('document.getElementById("notification-telegramToken").value'),'');
 await evaluate('document.querySelector(".global-notification-settings").scrollIntoView({block:"start"})');
 const settingsShot=await cdp('Page.captureScreenshot',{format:'png'});writeFileSync(join(root,'docs/design/global-notification-settings-qa.png'),Buffer.from(settingsShot.data,'base64'));
 // No notification-enabled job is executed: no real network delivery is attempted.
 const data={globalNotificationSettingsVerified:true,perJobNotificationCheckboxesOnly:true,secretsBlankOnReopen:true,version:appStatus.version,schema:appStatus.schemaVersion,operationColorsVerified:true,cloudProviderNamesVerified:true,responsiveLayouts:resizeLayouts,activeCompactLayouts:activeLayouts,cloudLiveProgress:true,cloudProgressSurvivesReload:true,compactLayouts:layouts,cloudQuotaUsesRing:true,cloudAboveLocalDisk:true,cloudManualRefreshForcesRequest:true,cloudQuotaLabelsDoNotOverlap:true,cloudSetupSeparateStep:true,encryptedConfigAutocreated:true,verificationProgressVisible:true,zeroReadLimitSaved:true,cloudQuotaVisibleWhenEnabled:true,automaticKeyGeneration:true,recoveryFileOverwriteRejected:true,secretsNeverReturned:true,architecture:process.env.NBM_ARCH??'x64',createdAndEdited:true,settingsPersisted:true,cloudQuotaHiddenWhenDisabled:true,viewport:'1024x700',fixtureOnly:true};
 writeFileSync(join(root,`docs/qa/advanced-ui-${data.architecture}.json`),JSON.stringify(data,null,2));console.log(JSON.stringify(data));
}finally{socket?.close();app.kill();}
