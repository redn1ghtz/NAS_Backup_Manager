import { OperationLabel } from "./OperationLabel";
import { StatusBadge } from "./StatusBadge";
import { StorageUsage } from "./StorageUsage";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Archive, CalendarClock, CheckCircle2, Database, List, AlertCircle, Info, RefreshCw } from "lucide-react";
import { Button } from "@/components/ui/button";
import { call, errorCode, scheduleLabel, size, displayPath, countLabel, isSourceChanged, type Job } from "./api";
import type { Translations } from "./JobForm";

interface RunSummary {
  state:string; startedAt:number; finishedAt:number|null; errorCode:string|null;
  created:number|null; updated:number|null; deleted:number|null; unchanged:number|null;
  createdBytes:number|null; updatedBytes:number|null; deletedBytes:number|null;
}
interface DashboardData {
  totalJobs:number; successfulJobs:number; failedJobs:number; pausedJobs:number;
  archiveFiles:number; archiveBytes:number; earliestExpiry:number|null;
  lastRun:RunSummary|null;
  recent:{id:number;name:string;path:string;kind:string;state:string;bytes:number;happenedAt:number}[];
}
export default function Dashboard({children,jobs,selected,t,language,disabled,desktop,onSelect,onPage,onSchedule}: {
  children:ReactNode;jobs:Job[];selected:Job|undefined;t:Translations;language:string;disabled:boolean;desktop:boolean;
  onSelect:(id:string)=>void;onPage:(page:"archive"|"schedule"|"history")=>void;onSchedule:(job:Job)=>void;
}) {
  const [data,setData]=useState<DashboardData>();
  const [capacityResult,setCapacityResult]=useState<{id:string;value:[number,number]}>();
  const capacity=capacityResult?.id===selected?.id?capacityResult?.value:undefined;
  const [cloud,setCloud]=useState<{id:string;total:number|null;used:number|null;free:number|null}>();
  const [cloudError,setCloudError]=useState("");const [cloudMeasuring,setCloudMeasuring]=useState(false);
  const cloudRequest=useRef(0);const cloudQueue=useRef<Promise<void>>(Promise.resolve());
  const [error,setError]=useState("");const [capacityError,setCapacityError]=useState("");
  const [measuring,setMeasuring]=useState(false);
  const describe=(code:string)=>t.errors[code as keyof typeof t.errors]??t.errors.filesystem;
  const date=(value:number|null)=>value===null?"—":new Date(value*1000).toLocaleString(language);
  useEffect(()=>{if(!desktop)return;let cancelled=false;setError("");
    void call<DashboardData>({action:"dashboard",id:selected?.id??null}).then(value=>{if(!cancelled)setData(value);}).catch(e=>{if(!cancelled)setError(errorCode(e));});
    return()=>{cancelled=true;};
  },[desktop,jobs,selected?.id]);
  useEffect(()=>{setData(undefined);},[selected?.id]);
  const capacityRequest=useRef(0);
  const capacityQueue=useRef<Promise<void>>(Promise.resolve());
  const selectedId=selected?.id;
  const measure=useCallback(async()=>{
    if(!desktop||!selectedId)return;
    const request=++capacityRequest.current;
    setMeasuring(true);setCapacityError("");
    const previous=capacityQueue.current;
    let release!:()=>void;capacityQueue.current=new Promise<void>(resolve=>{release=resolve;});
    await previous;
    if(request!==capacityRequest.current){release();return;}
    try{const value=await call<[number,number]>({action:"capacity",id:selectedId});if(request===capacityRequest.current)setCapacityResult({id:selectedId,value});}
    catch(e){if(request===capacityRequest.current){setCapacityResult(undefined);setCapacityError(errorCode(e));}}
    finally{release();if(request===capacityRequest.current)setMeasuring(false);}
  },[desktop,selectedId]);
  useEffect(()=>{setCapacityResult(undefined);setCapacityError("");},[selectedId]);
  useEffect(()=>{
    void measure();
    const focus=()=>{if(document.visibilityState==="visible")void measure();};
    window.addEventListener("focus",focus);
    return()=>{capacityRequest.current++;setMeasuring(false);window.removeEventListener("focus",focus);};
  },[measure,selected?.lastRun,selected?.lastResult,selected?.revision]);
  const cloudProvider=selected?.advanced.cloudProvider==="dropbox"?"Dropbox":selected?.advanced.cloudProvider==="onedrive"?"OneDrive":"";
  const cloudEnabled=!!selected&&selected.advanced.cloudMode!=="off";
  const measureCloud=useCallback(async(force=false)=>{
    if(!desktop||!selectedId||!cloudEnabled)return;
    const request=++cloudRequest.current;setCloudMeasuring(true);setCloudError("");
    const previous=cloudQueue.current;let release!:()=>void;cloudQueue.current=new Promise<void>(r=>{release=r;});await previous;
    if(request!==cloudRequest.current){release();return;}
    try{const value=await call<{total:number|null;used:number|null;free:number|null}>({action:"cloudCapacity",id:selectedId,force});if(request===cloudRequest.current)setCloud({id:selectedId,...value});}
    catch(e){if(request===cloudRequest.current)setCloudError(errorCode(e));}
    finally{release();if(request===cloudRequest.current)setCloudMeasuring(false);}
  },[desktop,selectedId,cloudEnabled]);
  useEffect(()=>{setCloud(undefined);setCloudError("");},[selectedId,cloudEnabled,selected?.revision]);
  useEffect(()=>{void measureCloud();const refresh=()=>{if(document.visibilityState==="visible")void measureCloud();};window.addEventListener("focus",refresh);return()=>{cloudRequest.current++;setCloudMeasuring(false);window.removeEventListener("focus",refresh);};},[measureCloud,selected?.lastRun,selected?.lastResult,selected?.revision]);
  const run=data?.lastRun;
  return <>
    {error&&<p className="error" role="alert">{describe(error)}</p>}
    {data&&<div className="dashboard-status" role="status"><span className={data.failedJobs?"status-error":data.totalJobs>0&&data.successfulJobs===data.totalJobs&&data.pausedJobs===0?"status-success":"status-muted"}>{data.failedJobs?<AlertCircle/>:data.totalJobs>0&&data.successfulJobs===data.totalJobs&&data.pausedJobs===0?<CheckCircle2/>:<Info/>}{data.failedJobs?t.backupAttention:data.pausedJobs?t.backupPaused:data.totalJobs>0&&data.successfulJobs===data.totalJobs?t.backupHealthy:t.backupWaiting}</span><small>{t.completedJobs}: {data.successfulJobs} / {data.totalJobs}</small></div>}
    <div className="dashboard-layout"><div className="dashboard-top">{children}<section className="panel storage-panel" tabIndex={0}><h2><Database aria-hidden="true"/>{selected?.kind==="network"?t.network:t.storage}</h2>
      <label className="selection-label">{t.chooseJob}<select value={selected?.id??""} disabled={!desktop} onChange={e=>onSelect(e.target.value)}><option value="">—</option>{jobs.map(job=><option value={job.id} key={job.id}>{job.name}</option>)}</select></label>
      {cloudEnabled&&<div className="cloud-capacity storage-section"><div className="capacity-heading"><h3>{t.cloudQuota}{cloudProvider&&` · ${cloudProvider}`}</h3><Button variant="ghost" size="icon-sm" aria-label={`${t.cloudQuota}: ${t.refresh}`} title={`${t.refresh}. ${t.cloudQuotaHelp}`} disabled={!desktop||cloudMeasuring} onClick={()=>void measureCloud(true)}><RefreshCw aria-hidden="true"/></Button></div><p className="job-path" title={selected?.advanced.cloudRemote}>{selected?.advanced.cloudRemote.split(":").slice(1).join(":")}</p>
        {cloud&&cloud.id===selectedId?<><StorageUsage values={cloud} t={t} language={language}/>{cloud.total===null&&cloud.free===null&&<p>{t.cloudQuotaUnknown}</p>}</>:<p role="status">{cloudMeasuring?t.loading:cloudError?describe(cloudError):t.cloudQuotaUnknown}</p>}
        {cloudError&&cloud?.id===selectedId&&<p className="error" role="alert">{describe(cloudError)}</p>}
      </div>}
      <div className="local-capacity storage-section"><div className="capacity-heading"><h3>{selected?.kind==="network"?t.network:selected?.advanced.cloudMode==="primary"?t.localCacheStorage:t.localDiskStorage}</h3><Button variant="ghost" size="icon-sm" aria-label={`${t.storage}: ${t.refresh}`} title={t.refresh} disabled={!desktop||!selected||measuring} onClick={()=>void measure()}><RefreshCw aria-hidden="true"/></Button></div>
        {selected&&<p className="job-path" title={displayPath(selected.destination)}>{displayPath(selected.destination)}</p>}
        {capacity?<StorageUsage values={{total:capacity[0],used:capacity[0]-capacity[1],free:capacity[1]}} t={t} language={language}/>:<p>{selected?measuring?t.loading:t.storagePending:t.storageDescription}</p>}
        {capacityError&&<p className="error" role="alert">{describe(capacityError)}</p>}
      </div>
    </section></div>
    <div className="utilities dashboard-utilities">
      <section className="panel" tabIndex={0}><h2><List aria-hidden="true"/>{t.lastSync}</h2><p>{selected?.name??t.allJobs}</p>{run?<><p>{date(run.startedAt)}</p><p><StatusBadge state={run.state} errorCode={run.errorCode} t={t}/></p>{run.errorCode&&<p className={(isSourceChanged(run.errorCode)||run.errorCode==="cancelled")?"message":"error"}>{describe(run.errorCode)}</p>}<dl className="sync-stats">{(["created","updated","deleted","unchanged"] as const).map(key=><div key={key}><dt className={run.state==="succeeded"?"stage-status status-success":"stage-status status-muted"}>{run.state==="succeeded"?<CheckCircle2 aria-hidden="true"/>:<Info aria-hidden="true"/>}{t[key]}</dt><dd>{run[key]??"—"}{key!=="unchanged"&&run[`${key}Bytes`]!==null&&run[`${key}Bytes`]!>0&&` (${size(run[`${key}Bytes`]!,language)})`}</dd></div>)}<div><dt className={run.state==="succeeded"?"stage-status status-success":"stage-status status-muted"}>{run.state==="succeeded"?<CheckCircle2 aria-hidden="true"/>:<Info aria-hidden="true"/>}{t.duration}</dt><dd>{run.finishedAt===null?"—":`${Math.max(0,run.finishedAt-run.startedAt)} ${t.seconds}`}</dd></div></dl></>:<p>{!desktop?t.unknownState:data?t.noBackup:t.loading}</p>}</section>
      <section className="panel" tabIndex={0}><div className="panel-heading"><h2><Archive aria-hidden="true"/>{t.archive}</h2><Button size="icon-lg" variant="ghost" aria-label={t.archive} title={t.archive} onClick={()=>onPage("archive")}>{">"}</Button></div><p className="archive-value">{data?`${countLabel(data.archiveFiles,t.fileCount,t.locale)} (${size(data.archiveBytes,language)})`:"—"}</p><p>{selected?selected.archiveMode==="none"?t.keepNone:`${t.archive}: ${countLabel(selected.retentionDays,t.dayCount,t.locale)}`:t.chooseJob}</p><p>{t.earliestExpiry}: {data?date(data.earliestExpiry):"—"}</p><p className="storage-note" title={t.archiveCountHelp}>{t.archiveCountHelp}</p></section>
      <section className="panel" tabIndex={0}><div className="panel-heading"><h2><CalendarClock aria-hidden="true"/>{t.schedule}</h2>{selected&&<Button variant="outline" role="switch" aria-checked={selected.scheduleEnabled} aria-label={t.schedule} title={!selected.scheduleAutomatic?t.manualScheduleHelp:selected.scheduleEnabled?t.disableSchedule:t.enableSchedule} className="schedule-switch" disabled={disabled||!selected.enabled||!selected.scheduleAutomatic} onClick={()=>onSchedule(selected)}><span aria-hidden="true"/></Button>}</div><p>{selected?scheduleLabel(selected,t):t.scheduleDescription}</p><p>{selected?selected.schedulePending?t.schedulePending:selected.scheduleEnabled?t.scheduleOn:t.scheduleOff:t.chooseJob}</p>{selected?.scheduleAutomatic&&<p>{selected.missedRun==="skip"?t.skip:t.runOnce}</p>}<Button variant="ghost" onClick={()=>onPage("schedule")}>{t.schedule}</Button></section>
    </div>
    <section className="panel recent-panel"><div className="panel-heading"><h2><List aria-hidden="true"/>{t.recentOperations}</h2><Button variant="ghost" onClick={()=>onPage("history")}>{t.allOperations} {">"}</Button></div><div className="table-scroll" tabIndex={0} role="region" aria-label={t.recentOperations}><table><thead><tr><th>{t.time}</th><th>{t.name}</th><th>{t.kind}</th><th>{t.path}</th><th>{t.bytes}</th><th>{t.fileStatus}</th></tr></thead><tbody>{data?.recent.map(item=><tr key={item.id}><td>{date(item.happenedAt)}</td><td>{item.name}</td><td><OperationLabel kind={item.kind} t={t}/></td><td>{item.path}</td><td>{size(item.bytes,language)}</td><td><StatusBadge state={item.state} kind={item.kind} t={t}/></td></tr>)}</tbody></table>{data&&!data.recent.length&&<p>{t.noHistory}</p>}</div></section>
    </div>
  </>;
}
