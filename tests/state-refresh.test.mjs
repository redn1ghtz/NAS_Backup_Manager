import {test} from 'node:test';
import assert from 'node:assert/strict';
import {setImmediate as flush} from 'node:timers/promises';
import {observeState,pollVisible} from '../apps/desktop/src/stateRefresh.ts';

test('active progress polling sleeps while hidden and refreshes on reopening',async t=>{
 t.mock.timers.enable({apis:['setTimeout']});
 let visible=false,calls=0;
 const observer=pollVisible({poll:async()=>{calls++;},isVisible:()=>visible,onError:assert.fail});
 t.after(()=>observer.stop());t.mock.timers.tick(60000);await flush();assert.equal(calls,0);
 visible=true;observer.wake();await flush();assert.equal(calls,1);
 t.mock.timers.tick(1000);await flush();assert.equal(calls,2);
 visible=false;observer.wake();t.mock.timers.tick(60000);await flush();assert.equal(calls,2);
 visible=true;observer.wake();await flush();assert.equal(calls,3);
 observer.stop();t.mock.timers.tick(60000);await flush();assert.equal(calls,3);
});

test('progress polls coalesce focus events and retry errors without overlap',async t=>{
 t.mock.timers.enable({apis:['setTimeout']});
 let calls=0,errors=0,release;
 const observer=pollVisible({poll:()=>{calls++;return calls===1?new Promise(resolve=>{release=resolve;}):calls===2?Promise.reject(Error('temporary')):Promise.resolve();},isVisible:()=>true,onError:()=>{errors++;}});
 t.after(()=>observer.stop());observer.wake();observer.wake();assert.equal(calls,1);
 release();await flush();t.mock.timers.tick(0);await flush();assert.equal(calls,2);assert.equal(errors,1);
 t.mock.timers.tick(1000);await flush();assert.equal(calls,3);
});

test('refreshes changed metadata only; pauses hidden and stops on disposal',async t=>{
 t.mock.timers.enable({apis:['setTimeout']});
 let visible=true,checks=0,refreshes=0,changed=true;
 const observer=observeState({changed:async()=>{checks++;return changed;},refresh:async()=>{refreshes++;},isActive:()=>visible,onError:assert.fail});
 t.after(()=>observer.stop());await flush();assert.equal(refreshes,1);
 changed=false;t.mock.timers.tick(5000);await flush();assert.equal(checks,2);assert.equal(refreshes,1);
 visible=false;t.mock.timers.tick(5000);await flush();assert.equal(checks,2);
 visible=true;changed=true;observer.wake();await flush();assert.equal(refreshes,2);
 observer.stop();t.mock.timers.tick(10000);await flush();assert.equal(checks,3);
});

test('coalesces focus events without overlapping checks',async t=>{
 let release,checks=0,refreshes=0;
 const observer=observeState({changed:()=>{checks++;return checks===1?new Promise(resolve=>{release=resolve;}):Promise.resolve(false);},refresh:async()=>{refreshes++;},isActive:()=>true,onError:assert.fail});
 t.after(()=>observer.stop());observer.wake();observer.wake();observer.wake();assert.equal(checks,1);
 release(true);await flush();assert.equal(checks,2);assert.equal(refreshes,1);
});

test('retries a failed refresh even when the change token was already consumed',async t=>{
 t.mock.timers.enable({apis:['setTimeout']});
 let checks=0,refreshes=0,errors=0;
 const observer=observeState({changed:async()=>++checks===1,refresh:async()=>{if(++refreshes===1)throw Error('temporary');},isActive:()=>true,onError:()=>{errors++;}});
 t.after(()=>observer.stop());await flush();assert.equal(errors,1);
 t.mock.timers.tick(5000);await flush();assert.equal(refreshes,2);
});
