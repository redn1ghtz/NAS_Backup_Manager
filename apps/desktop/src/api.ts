import type en from "./locales/en.json";
import { invoke } from "@tauri-apps/api/core";

export interface JobDraft { id: string | null; revision: number | null; name: string; source: string; destination: string; kind: "local" | "network"; scheduleAutomatic: boolean; wakeComputer: boolean; deletionGuardEnabled: boolean; deletionGuardMinFiles: number; retentionDays: number; archiveMode: "days"|"none"; scheduleKind: "daily"|"minutes"|"hours"|"days"|"startup"; scheduleEvery: number; dailyMinute: number; missedRun: "skip" | "run_once_when_available" }
export interface Job extends Omit<JobDraft,"id"|"revision"> { id:string; revision:number; enabled:boolean; scheduleEnabled:boolean; schedulePending:boolean; scheduleAnchor:string; currentPath:string; destinationLayout:"isolated"|"direct"; lastResult:string|null; lastError:string|null; lastRun:number|null }
export interface Preview {id:string; created:number; updated:number; deleted:number; unchanged:number; bytes:number; massDeletion:boolean; databaseWarning:boolean; archiveDisabled:boolean}
export interface Operation {path:string;kind:string;bytes:number}
export interface ArchiveEntry {id:string;path:string;archivedAt:number;expiresAt:number;bytes:number;reason:string}
export interface HistoryEntry {id:string;jobId:string;name:string;state:string;startedAt:number;finishedAt:number|null;errorCode:string|null;triggerKind:string}
export interface ApiFailure {code:string;detail:string}
export function call<T>(request:Record<string,unknown>):Promise<T>{return invoke<T>("backup_command",{request});}
export function errorCode(error:unknown):string {return typeof error === "object" && error && "code" in error ? String(error.code) : String(error);}
export function size(bytes:number,locale:string){const units=["byte","kilobyte","megabyte","gigabyte","terabyte"] as const;const index=bytes>0?Math.min(4,Math.floor(Math.log(bytes)/Math.log(1024))):0;return new Intl.NumberFormat(locale,{style:"unit",unit:units[index],maximumFractionDigits:1}).format(bytes/1024**index);}

type PluralForms={one:string;few:string;many:string;other:string};
const pluralRules=new Map<string,Intl.PluralRules>();
export function countLabel(count:number,forms:PluralForms,locale:string){
 let rules=pluralRules.get(locale);if(!rules){rules=new Intl.PluralRules(locale);pluralRules.set(locale,rules);}
 const category=rules.select(count);const key=category in forms?category as keyof PluralForms:"other";
 return forms[key].replace("{count}",String(count));
}
export function scheduleLabel(job: Pick<JobDraft,"scheduleKind"|"scheduleEvery"|"dailyMinute"> & {scheduleAutomatic?:boolean},t:typeof en) {
 if(job.scheduleAutomatic===false)return t.manualOnly;
 if(job.scheduleKind==="startup")return t.startup;
 const time=`${String(Math.floor(job.dailyMinute/60)).padStart(2,"0")}:${String(job.dailyMinute%60).padStart(2,"0")}`;
 if(job.scheduleKind==="daily")return `${t.daily} · ${time}`;
 const forms=job.scheduleKind==="days"?t.scheduleDays:job.scheduleKind==="hours"?t.scheduleHours:t.scheduleMinutes;
 const interval=countLabel(job.scheduleEvery,forms,t.locale);
 return job.scheduleKind==="days"?`${interval} · ${time}`:interval;
}
export function displayPath(path:string){
 if(path.toLowerCase().startsWith("\\\\?\\unc\\"))return "\\\\"+path.slice(8);
 if(path.startsWith("\\\\?\\"))return path.slice(4);
 return path;
}
export function greetingKey(hour:number):"greetingMorning"|"greetingDay"|"greetingEvening"|"greetingNight" {
 return hour>=6&&hour<12?"greetingMorning":hour>=12&&hour<18?"greetingDay":hour>=18&&hour<23?"greetingEvening":"greetingNight";
}

export interface FileEvent {id:number;name:string;path:string;kind:string;state:string;bytes:number;happenedAt:number}

export interface LiveProgress {jobId:string;name:string;phase:string;completed:number;total:number;copiedBytes:number;verifiedBytes:number;etaSeconds:number|null;totalBytes:number;currentPath:string|null;updatedAt:number;executionId?:string;cancelRequested?:boolean}
export function isSourceChanged(code:string|null|undefined){return code==="stale_preview"||code==="file_changed_during_copy";}

export function sameJobPage(previous:Job[],next:Job[]){return JSON.stringify(previous)===JSON.stringify(next);}
export function progressPercent(activity:Pick<LiveProgress,"phase"|"totalBytes"|"copiedBytes"|"verifiedBytes"|"total"|"completed">){
 if(activity.phase==="scanning")return undefined;
 const total=activity.totalBytes>0?activity.totalBytes*2:activity.total;
 const completed=activity.totalBytes>0?activity.copiedBytes+(activity.verifiedBytes??0):activity.completed;
 return total>0?Math.max(0,Math.min(99,Math.floor(completed/total*100))):undefined;
}
export function remainingLabel(seconds:number|null|undefined,t:typeof en){
 if(seconds==null||!Number.isFinite(seconds)||seconds<1)return t.etaPending;
 const whole=Math.ceil(seconds);const hours=Math.floor(whole/3600),minutes=Math.floor(whole%3600/60),rest=whole%60;
 const value=(hours?t.etaHours:minutes?t.etaMinutes:t.etaSeconds).replace("{hours}",String(hours)).replace("{minutes}",String(minutes)).replace("{seconds}",String(rest));
 return t.remainingApprox.replace("{time}",value);
}
