// Explicit visual QA entry only. Vite production builds index.html, not this page.
import React, {useState} from "react";
import {mockIPC} from "@tauri-apps/api/mocks";
import Dashboard from "../src/Dashboard";
import {App} from "../src/App";
import type {Job} from "../src/api";
import {createRoot} from "react-dom/client";
import {StatusBadge} from "../src/StatusBadge";
import NetworkAccess from "../src/NetworkAccess";
import JobForm from "../src/JobForm";
import ru from "../src/locales/ru.json";
import {greetingKey,scheduleLabel} from "../src/api";
import "../src/styles.css";

const appMode=new URLSearchParams(location.search).get("mode")==="app";
const baseJob:Job={id:"qa-a",revision:1,name:"QA auto-refresh",source:"C:\\fixture-source",destination:"D:\\fixture-dest",kind:"local",scheduleAutomatic:true,wakeComputer:false,retentionDays:30,archiveMode:"days",scheduleKind:"minutes",scheduleEvery:3,dailyMinute:180,missedRun:"skip",deletionGuardEnabled:true,deletionGuardMinFiles:10,enabled:true,scheduleEnabled:false,schedulePending:false,scheduleAnchor:"2026-10-19",currentPath:"D:\\fixture-dest",destinationLayout:"direct",lastRun:1700000000,lastError:null,lastResult:"succeeded"};
const manualMode=new URLSearchParams(location.search).has("manual");
const pendingMode=new URLSearchParams(location.search).has("pending");
let notifications=true;let active=new URLSearchParams(location.search).has("progress");
let appJobs=[{...baseJob,scheduleAutomatic:!manualMode,schedulePending:pendingMode,enabled:!pendingMode}],dirty=true;
if(new URLSearchParams(location.search).has("stale"))appJobs=appJobs.map(job=>({...job,lastResult:"interrupted",lastError:"stale_preview"}));
const commands:string[]=[];
if(appMode){
 Object.assign(window,{isTauri:true});
 mockIPC((command,args)=>{
  if(command==="state_changed"){const changed=dirty;dirty=false;return changed;}
  if(command==="get_app_status")return {version:"0.1.0",schemaVersion:10,notificationsEnabled:notifications,jobCount:appJobs.length,language:"ru",executionAvailable:true,wakeSupported:true};
  if(command==="set_notifications"){notifications=!!args.enabled;return;}
  if(command!=="backup_command")throw Error(`Unexpected command: ${command}`);
  const request=args.request as {action:string;enabled?:boolean};commands.push(request.action);
  if(request.action==="activity")return active?[{jobId:baseJob.id,name:baseJob.name,phase:"copying",completed:2,total:5,copiedBytes:50*1024*1024,verifiedBytes:0,etaSeconds:120,totalBytes:100*1024*1024,currentPath:"Документы / большой файл.bin",updatedAt:Date.now()/1000}]:[];
  if(request.action==="listJobs")return appJobs.map(job=>({...job}));
  if(request.action==="capacity")return [100000,80000];
  if(request.action==="dashboard")return {totalJobs:appJobs.length,successfulJobs:appJobs.filter(job=>job.lastResult==="succeeded").length,failedJobs:0,pausedJobs:0,archiveFiles:0,archiveBytes:0,earliestExpiry:null,lastRun:appJobs[0]?{state:appJobs[0].lastResult,startedAt:appJobs[0].lastRun,finishedAt:appJobs[0].lastRun,errorCode:appJobs[0]?.lastError,created:0,updated:1,deleted:0,unchanged:1,createdBytes:0,updatedBytes:35,deletedBytes:0}:null,recent:[]};
  if(request.action==="preview")return {id:"qa-preview",created:1,updated:0,deleted:0,unchanged:0,bytes:35,massDeletion:false,databaseWarning:false,archiveDisabled:false};
  if(request.action==="operations")return [];
  if(request.action==="run"){appJobs=appJobs.map(job=>({...job,enabled:true,scheduleEnabled:job.scheduleAutomatic&&job.schedulePending,schedulePending:false}));dirty=true;return {result:{id:"qa-run"},scheduleError:null};}
  if(request.action==="schedule"){appJobs=appJobs.map(job=>({...job,scheduleEnabled:!!request.enabled}));dirty=true;return;}
  if(request.action==="remove"){appJobs=[];dirty=true;return;}
  throw Error(`Unexpected action: ${request.action}`);
 },{shouldMockEvents:true});
}
function AppFixture(){
 const [actions,setActions]=useState("");
 return <><aside style={{padding:8}}>UI QA: mock IPC, no real files or OS tasks. <button onClick={()=>{appJobs=appJobs.map(job=>({...job,lastRun:1700000180}));dirty=true;}}>Simulate headless backup</button><button onClick={()=>{active=false;dirty=true;}}>Finish background sync</button><button onClick={()=>setActions(commands.join(","))}>Show IPC calls</button><output>{actions}</output></aside><App/></>;
}
const dashboard=new URLSearchParams(location.search).get("mode")==="dashboard";
function DashboardFixture(){
 const [selected,setSelected]=useState("qa-a");const [calls,setCalls]=useState<string[]>([]);
 const [jobs,setJobs]=useState<Job[]>(()=>["a","b"].map(id=>({id:`qa-${id}`,revision:1,name:`QA ${id}`,source:"C:\\fixture-source",destination:`D:\\fixture-${id}`,kind:"local",retentionDays:30,archiveMode:"days",scheduleKind:"daily",scheduleEvery:1,dailyMinute:180,missedRun:"skip",scheduleAutomatic:true,wakeComputer:false,deletionGuardEnabled:true,deletionGuardMinFiles:10,enabled:true,scheduleEnabled:false,schedulePending:false,scheduleAnchor:"2026-10-19",currentPath:`D:\\fixture-${id}`,destinationLayout:"direct",lastRun:null,lastError:null,lastResult:null})));
 mockIPC((command,args)=>{
  if(command!=="backup_command")throw Error("Unexpected command");
  const request=args.request as {action:string;id:string};
  if(request.action==="capacity"){setCalls(previous=>[...previous,request.id]);return request.id==="qa-a"?[10000,8000]:[20000,10000];}
  if(request.action==="dashboard")return {totalJobs:2,successfulJobs:0,failedJobs:0,pausedJobs:0,archiveFiles:0,archiveBytes:0,earliestExpiry:null,lastRun:null,recent:[]};
  throw Error("Unexpected request");
 });
 return <main><h1>UI QA: mock IPC only — no disks or NAS accessed</h1><p role="status">Capacity requests: {calls.join(", ")}</p><button onClick={()=>setJobs(previous=>previous.map(job=>({...job,lastRun:(job.lastRun??0)+1})))}>Simulate completed sync refresh</button><Dashboard jobs={jobs} selected={jobs.find(job=>job.id===selected)} t={ru} language="ru" desktop disabled={false} onSelect={setSelected} onPage={()=>{}} onSchedule={()=>{}}><section className="panel"><h2>QA tasks</h2><p>Isolated dashboard fixture</p></section></Dashboard></main>;
}
const form=new URLSearchParams(location.search).get("mode")==="form";
const network=new URLSearchParams(location.search).get("mode")==="network";
createRoot(document.getElementById("root")!).render(<React.StrictMode>{appMode?<AppFixture/>:dashboard?<DashboardFixture/>:network?<main><p>UI QA: connection mode controls only; no NAS connection is made.</p><NetworkAccess path={String.raw`\\fixture.invalid\share\ddd`} t={ru} loadSaved={false}/></main>:form?<JobForm job={null} t={ru} wakeSupported onClose={()=>location.assign("/tests/ui-check.html")} onSaved={()=>{throw new Error("No backend in the QA fixture");}}/>:<main><h1>{ru[greetingKey(new Date().getHours())]}</h1><p>UI QA: isolated component examples; these are not backup results.</p><a href="?mode=form">Форма новой задачи</a> · <a href="?mode=network">Гостевой доступ</a><section className="panel"><h2>{ru.lastSync}</h2><p><StatusBadge state="succeeded" t={ru}/></p><p><StatusBadge state="failed" t={ru}/></p><p><StatusBadge state="running" t={ru}/></p><table><thead><tr><th>{ru.kind}</th><th>{ru.fileStatus}</th></tr></thead><tbody>{["create","update","archive_remove","unchanged","permanent_remove"].map(kind=><tr key={kind}><td>{kind}</td><td><StatusBadge state="committed" kind={kind} t={ru}/></td></tr>)}</tbody></table><p>{scheduleLabel({scheduleKind:"minutes",scheduleEvery:2,dailyMinute:180},ru)}</p></section></main>}</React.StrictMode>);
