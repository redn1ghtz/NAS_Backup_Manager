import { test } from 'node:test';
import assert from 'node:assert/strict';
import ru from '../apps/desktop/src/locales/ru.json' with { type: 'json' };
import en from '../apps/desktop/src/locales/en.json' with { type: 'json' };
import { scheduleLabel, countLabel, displayPath, greetingKey, isSourceChanged, progressPercent, remainingLabel, sameJobPage } from '../apps/desktop/src/api.ts';

test('Russian schedule counts include singular, few and 11–14 exceptions', () => {
  for (const [count,expected] of [[1,'Каждую 1 минуту'],[2,'Каждые 2 минуты'],[5,'Каждые 5 минут'],[11,'Каждые 11 минут'],[14,'Каждые 14 минут'],[21,'Каждую 21 минуту'],[22,'Каждые 22 минуты'],[111,'Каждые 111 минут']]) {
    assert.equal(scheduleLabel({scheduleKind:'minutes',scheduleEvery:count,dailyMinute:180},ru),expected);
  }
  assert.equal(scheduleLabel({scheduleKind:'hours',scheduleEvery:2,dailyMinute:180},ru),'Каждые 2 часа');
  assert.equal(scheduleLabel({scheduleKind:'days',scheduleEvery:3,dailyMinute:185},ru),'Каждые 3 дня · 03:05');
  assert.equal(scheduleLabel({scheduleKind:'minutes',scheduleEvery:1,dailyMinute:0},en),'Every 1 minute');
  assert.equal(scheduleLabel({scheduleKind:'hours',scheduleEvery:2,dailyMinute:0},en),'Every 2 hours');
});
test('Archive file/day counts agree with the number', () => {
  for (const [n,text] of [[0,'0 файлов'],[1,'1 файл'],[2,'2 файла'],[12,'12 файлов'],[21,'21 файл']]) assert.equal(countLabel(n,ru.fileCount,'ru'),text);
  assert.equal(countLabel(2,ru.dayCount,'ru'),'2 дня');
});
test('Display strips only Windows extended path prefixes', () => {
  assert.equal(displayPath(String.raw`\\?\D:\Example`),String.raw`D:\Example`);
  assert.equal(displayPath(String.raw`\\?\UNC\192.0.2.10\backup\example`),String.raw`\\192.0.2.10\backup\example`);
  assert.equal(displayPath('/home/user/a?b'),'/home/user/a?b');
  assert.equal(displayPath(String.raw`D:\Example`),String.raw`D:\Example`);
});
test('Greeting follows local day boundaries', () => {
  for (const [hour,key] of [[0,'greetingNight'],[5,'greetingNight'],[6,'greetingMorning'],[11,'greetingMorning'],[12,'greetingDay'],[17,'greetingDay'],[18,'greetingEvening'],[22,'greetingEvening'],[23,'greetingNight']]) assert.equal(greetingKey(hour),key);
});

test("Manual-only mode overrides any stored recurrence",()=>{
 assert.equal(scheduleLabel({scheduleAutomatic:false,scheduleKind:"minutes",scheduleEvery:10,dailyMinute:180},ru),ru.manualOnly);
 assert.equal(scheduleLabel({scheduleAutomatic:false,scheduleKind:"daily",scheduleEvery:1,dailyMinute:180},en),en.manualOnly);
});

test('Expected source changes are distinct from backup failures',()=>{
 assert.equal(isSourceChanged('stale_preview'),true);
 assert.equal(isSourceChanged('file_changed_during_copy'),true);
 for(const code of [null,undefined,'source_unavailable','mass_deletion','disk_full'])assert.equal(isSourceChanged(code),false);
 for(const t of [ru,en]){assert.equal(t.errors.stale_preview,t.sourceChanged);assert.equal(t.errors.file_changed_during_copy,t.sourceChanged);assert.ok(t.notificationsHelp);}
});

test('Startup mode has an event label instead of an interval',()=>{
 assert.equal(scheduleLabel({scheduleAutomatic:true,scheduleKind:'startup',scheduleEvery:1,dailyMinute:180},ru),'При включении ПК');
 assert.equal(scheduleLabel({scheduleAutomatic:true,scheduleKind:'startup',scheduleEvery:1,dailyMinute:180},en),en.startup);
 assert.equal(scheduleLabel({scheduleAutomatic:false,scheduleKind:'startup',scheduleEvery:1,dailyMinute:180},ru),ru.manualOnly);
});

test('Overall progress includes readback and never shows completed while active',()=>{
 const p={phase:'copying',totalBytes:100,copiedBytes:100,verifiedBytes:0,total:1,completed:0};
 assert.equal(progressPercent(p),50);
 assert.equal(progressPercent({...p,phase:'verifying',verifiedBytes:50}),75);
 assert.equal(progressPercent({...p,phase:'finalizing',verifiedBytes:100,completed:1}),99);
 assert.equal(progressPercent({...p,phase:'scanning'}),undefined);
});
test('ETA is approximate, localized, and unknown rather than zero at completion',()=>{
 assert.equal(remainingLabel(125,ru),'Осталось ≈ 2 мин 5 с');
 assert.equal(remainingLabel(3661,en),'Remaining ≈ 1 h 1 min');
 for(const v of [null,undefined,0,NaN])assert.equal(remainingLabel(v,ru),ru.etaPending);
});
test('Progress-only SQLite commits preserve the same job page identity',()=>{
 const p=[{id:'a',lastResult:'running',lastRun:1}];assert.ok(sameJobPage(p,JSON.parse(JSON.stringify(p))));
 assert.equal(sameJobPage(p,[{...p[0],lastResult:'succeeded'}]),false);
});

test('Integrity progress uses verified bytes once and cloud checks stay indeterminate',()=>{
 const p={phase:'integrity',totalBytes:100,copiedBytes:0,verifiedBytes:50,total:2,completed:1};
 assert.equal(progressPercent(p),50);
 assert.equal(progressPercent({...p,verifiedBytes:100}),99);
 assert.equal(progressPercent({...p,phase:'cloudIntegrity'}),undefined);
});

test('Cloud percentage follows uploaded bytes without local readback weighting',()=>{
 const p={phase:'cloud',totalBytes:100,copiedBytes:40,verifiedBytes:0,total:4,completed:1};
 assert.equal(progressPercent(p),40);
 assert.equal(progressPercent({...p,phase:'cloudArchive',copiedBytes:80}),80);
 assert.equal(progressPercent({...p,copiedBytes:100}),99);
 assert.equal(progressPercent({...p,totalBytes:0}),undefined);
});
