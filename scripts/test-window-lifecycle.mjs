// Opt-in Windows regression against an isolated copy of the real release GUI.
// CDP is enabled only in this child; no user profile, NAS or notifications used.
import assert from 'node:assert/strict';
import { spawn, execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync, openSync, writeSync, closeSync, createReadStream, unlinkSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { createServer } from 'node:net';
import { resolve, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const packageFolder = process.env.NAS_BACKUP_TEST_PACKAGE ?? join(root,'portable/NAS Backup Manager');
const stayHidden = process.argv.includes('--stay-hidden');
const notifications = process.argv.includes('--notifications');
const cancelRun = process.argv.includes('--cancel');
const cycles = Number(process.argv.find(argument => argument.startsWith('--cycles='))?.split('=')[1] ?? 5);
assert(Number.isInteger(cycles) && cycles >= 1 && cycles <= 20, 'Use --cycles=1..20');
const fixture = mkdtempSync(join(root, 'target/window-lifecycle-'));
const executable = join(fixture, 'NAS Backup Manager.exe');
copyFileSync(join(packageFolder, 'NAS Backup Manager.exe'), executable);
copyFileSync(join(packageFolder, 'nas-backup-notify.exe'), join(fixture, 'nas-backup-notify.exe'));
writeFileSync(join(fixture, 'portable.flag'), 'disposable lifecycle QA');
const source = join(fixture, 'source'), destination = join(fixture, 'destination');
mkdirSync(source); mkdirSync(destination);
const buffer = Buffer.alloc(4 * 1024 * 1024, 0x5a);
const file = openSync(join(source, 'large.bin'), 'wx');
try { for (let i = 0; i < 128; i++) writeSync(file, buffer); } finally { closeSync(file); }
const server = createServer();
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const port = server.address().port;
await new Promise(resolve => server.close(resolve));
const app = spawn(executable, [], { cwd: fixture, windowsHide: true, stdio: 'ignore', env: {
  ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
}});
let socket;
const sleep = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, seconds = 30) {
  const deadline = Date.now() + seconds * 1000;
  while (Date.now() < deadline) { const value = await predicate(); if (value) return value; await sleep(100); }
  throw new Error('Lifecycle regression timed out');
}
const powershell = script => execFileSync('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', script], { windowsHide: true, encoding: 'utf8' }).trim();
const ownedProcess = `$p=Get-Process -Id ${app.pid} -ErrorAction Stop; if($p.Path -ne '${executable.replaceAll("'", "''")}'){throw 'Wrong process'}; `;
// Warm the controller before copying so PowerShell startup cannot consume the
// entire local-file transfer and turn reopening into a fresh-process test.
const controller = spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', `${ownedProcess}[Console]::WriteLine('ready'); while([Console]::ReadLine() -eq 'close'){[Console]::WriteLine($p.CloseMainWindow())}`], {windowsHide:true, stdio:['pipe','pipe','pipe']});
let controllerReply;
controller.stdout.on('data', chunk => { controllerReply?.(chunk.toString().trim()); controllerReply = undefined; });
const ready = new Promise(resolve => { controllerReply = resolve; });
await ready;
let reopened;
try {
  const target = await until(async () => {
    try { return (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find(t => t.type === 'page' && t.url.includes('tauri.localhost')); } catch { return null; }
  });
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let sequence = 0;
  const pending = new Map();
  socket.onmessage = event => { const data = JSON.parse(event.data); if (pending.has(data.id)) { pending.get(data.id)(data); pending.delete(data.id); } };
  async function evaluate(expression) {
    const id = ++sequence;
    const response = new Promise(resolve => pending.set(id, resolve));
    socket.send(JSON.stringify({ id, method: 'Runtime.evaluate', params: { expression, awaitPromise: true, returnByValue: true } }));
    let timer;
    const result = await Promise.race([response, new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('CDP timeout')), 30000); })]).finally(() => clearTimeout(timer));
    assert(!result.error && !result.result.exceptionDetails, JSON.stringify(result));
    return result.result.result.value;
  }
  const invoke = (command, args) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args ?? {})})`);
  const backup = request => invoke('backup_command', { request });
  await until(() => evaluate('Boolean(window.__TAURI_INTERNALS__?.invoke)'));
  await until(async () => { try { return await invoke('get_app_status'); } catch { return false; } });
  assert.equal((await invoke('get_app_status')).version,JSON.parse(readFileSync(join(root,'apps/desktop/package.json'),'utf8')).version,'GUI version must match the packaged release');
  await sleep(500); // Let the frontend's first-launch local migration/read requests settle.
  await invoke('set_notifications', { enabled: notifications });
  const job = await backup({ action: 'saveJob', draft: { name: 'Lifecycle QA', source, destination, kind: 'local', scheduleAutomatic: false, wakeComputer: false, deletionGuardEnabled: true, deletionGuardMinFiles: 10, retentionDays: 30, archiveMode: 'days', scheduleKind: 'daily', scheduleEvery: 1, dailyMinute: 180, missedRun: 'skip' } });
  if (cancelRun) {
    // Drive the real Preview/consent/Run flow so cancellation is tested while
    // React's normal manual-run busy state is active. Observe, never fake IPC.
    await evaluate(`window.dispatchEvent(new Event('focus'));true`);
    await until(() => evaluate(`(()=>{const button=[...document.querySelectorAll('.job-row button')].find(b=>b.textContent.trim()==='Предпросмотр');if(!button||button.disabled)return false;button.click();return true;})()`));
    await until(() => evaluate(`(()=>{const checkbox=document.querySelector('[role="dialog"] input[type="checkbox"]');if(!checkbox)return false;if(!checkbox.checked)checkbox.click();return true;})()`));
    await until(() => evaluate(`(()=>{const button=[...document.querySelectorAll('[role="dialog"] button')].find(b=>b.textContent.trim()==='Синхронизировать сейчас');if(!button||button.disabled)return false;button.click();return true;})()`));
  } else {
    const plan = await backup({ action: 'preview', id: job.id });
    await evaluate(`window.lifecycleResult=null; window.__TAURI_INTERNALS__.invoke('backup_command',${JSON.stringify({ request: { action: 'run', id: job.id, previewId: plan.id, consent: true } })}).then(value=>window.lifecycleResult={ok:true,value},error=>window.lifecycleResult={ok:false,error}); true`);
  }
  await until(async () => (await backup({ action: 'activity' })).some(p => p.phase === 'copying'));
  console.log('Copy running; closing the isolated GUI.');
  const closed = new Promise(resolve => { controllerReply = resolve; });
  controller.stdin.write('close\n');
  assert.equal(await closed, 'True');
  await until(async () => !(await invoke('plugin:window|is_visible', {label:'main'})), 5);
  assert((await backup({ action: 'activity' })).length, 'Copy finished before close could be tested');
  if (stayHidden) {
    await until(() => app.exitCode !== null, 90);
    assert.equal(app.exitCode, 0);
    const state = execFileSync('python', ['-c', "import sqlite3,sys; db=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True); print(db.execute(\"SELECT state FROM runs WHERE trigger_kind='manual' ORDER BY started_at DESC LIMIT 1\").fetchone()[0])", join(fixture,'data/state.sqlite')], {windowsHide:true,encoding:'utf8'}).trim();
    assert.equal(state, 'succeeded');
  } else {
    for (let cycle = 1; cycle <= cycles; cycle++) {
      console.log(`Hidden copy is alive; reopening cycle ${cycle}/${cycles}.`);
      reopened = spawn(executable, [], { cwd: fixture, windowsHide: true, stdio: 'ignore' });
      await until(() => reopened.exitCode !== null, 10);
      assert.equal(reopened.exitCode, 0, 'Repeated launch failed');
      await until(() => invoke('plugin:window|is_visible', {label:'main'}), 5);
      if (cycle < cycles) {
        assert((await backup({ action: 'activity' })).length, 'Transfer must remain active across cycles');
        const closed = new Promise(resolve => { controllerReply = resolve; });
        controller.stdin.write('close\n');
        assert.equal(await closed, 'True');
        await until(async () => !(await invoke('plugin:window|is_visible', {label:'main'})), 5);
      }
    }
    if (cancelRun) {
      const id = ++sequence;
      const screenshot = new Promise(resolve => pending.set(id, resolve));
      socket.send(JSON.stringify({id,method:'Page.captureScreenshot',params:{format:'png'}}));
      const capture = await screenshot;
      if (capture.result?.data) writeFileSync(join(root,packageFolder.endsWith(' x86')?'docs/qa/sync-cancel-x86.png':'docs/qa/sync-cancel.png'),Buffer.from(capture.result.data,'base64'));
      await until(() => evaluate(`(()=>{const button=document.querySelector('.sync-progress button');if(!button||button.disabled)return false;button.click();return true;})()`), 5);
      console.log('Real Cancel button clicked.');
    }
    if (cancelRun) {
      await until(() => evaluate(`document.querySelector('.job-result')?.textContent.includes('Отменено')`),90);
    } else {
      const outcome = await until(() => evaluate('window.lifecycleResult'), 90);
      assert(outcome.ok, JSON.stringify(outcome));
    }
    console.log('Worker outcome received.');
    const jobs = await backup({ action: 'listJobs', offset: 0 });
    assert.equal(jobs[0].lastResult, cancelRun ? 'interrupted' : 'succeeded');
    if (cancelRun) {
      assert.equal(jobs[0].lastError, 'cancelled');
      await until(() => evaluate(`document.querySelector('.job-result')?.textContent.includes('Отменено')`));
      assert.equal(existsSync(join(destination,'large.bin')),false,'Cancelled create must not commit a partial destination');
    }
    await sleep(3000);
    assert.equal(app.exitCode, null, 'Completion must not exit the reopened GUI');
    assert.equal(await invoke('plugin:window|is_visible', {label:'main'}), true, 'Completion must preserve the open window');
  }
  const digest = async path => { const hash = createHash('sha256'); for await (const chunk of createReadStream(path)) hash.update(chunk); return hash.digest('hex'); };
  const notificationFailures = execFileSync('python', ['-c', "import sqlite3,sys; db=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True); print(db.execute(\"SELECT count(*) FROM audit_events WHERE kind='notification' AND state='failed'\").fetchone()[0])", join(fixture,'data/state.sqlite')], {windowsHide:true,encoding:'utf8'}).trim();
  assert.equal(notificationFailures, '0', 'Packaged tray/notification sidecar must work in the GUI');
  if (!cancelRun) assert.equal(await digest(join(source, 'large.bin')), await digest(join(destination, 'large.bin')));
  if (!stayHidden) {
    console.log('Closing the idle test window.');
    const closed = new Promise(resolve => { controllerReply = resolve; });
    controller.stdin.write('close\n');
    assert.equal(await closed, 'True');
    await until(() => app.exitCode !== null);
  }
  console.log(cancelRun ? 'PASS: real UI Cancel works during manual-run busy state after reopen; partial file absent; neutral cancelled status; GUI stays open.' : stayHidden ? 'PASS: copy completes with GUI hidden; verified destination; process exits after completion.' : `PASS: ${cycles} close/reopen cycles reused the original process; copy verified and succeeded; idle close exits.`);
  console.log(`Disposable fixture: ${fixture}`);
  unlinkSync(join(source, 'large.bin')); if(existsSync(join(destination, 'large.bin')))unlinkSync(join(destination, 'large.bin'));
} finally {
  socket?.close();
  controller.stdin.end(); controller.kill();
  if (app.exitCode === null) app.kill(); // Only the exact test child, never the user application.
  if (reopened && reopened.exitCode === null) reopened.kill();
  if (notifications) execFileSync(join(root, 'portable/NAS Backup Manager/nas-backup-notify.exe'), ['--register', join(root, 'portable/NAS Backup Manager/NAS Backup Manager.exe')], {windowsHide:true});
}
