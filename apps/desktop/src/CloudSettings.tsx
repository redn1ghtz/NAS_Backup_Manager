import { useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import { call, errorCode, type JobDraft } from "./api";
import type { Translations } from "./JobForm";

interface Connection {connectionId:string;provider:string;config:string;connected:boolean;executable:string}
export default function CloudSettings({draft,onChange,t,onPending,onReady}:{draft:JobDraft;onChange:React.Dispatch<React.SetStateAction<JobDraft>>;t:Translations;onPending:(value:boolean)=>void;onReady:(value:boolean)=>void}) {
 const a=draft.advanced;
 const unsupported=Boolean(a.cloudProvider&&!["onedrive","dropbox"].includes(a.cloudProvider));
 const [provider,setProvider]=useState(unsupported?"onedrive":a.cloudProvider||(a.rcloneConfig?"legacy":"onedrive"));
 const [connected,setConnected]=useState(!unsupported&&!a.cloudConnection&&Boolean(a.rcloneConfig));
 const [pending,setPending]=useState(false);const [error,setError]=useState(unsupported?"cloud_provider_unavailable":"");
 const authorization=useRef<string|null>(null);
 const cancelled=useRef(false);
 const path=a.cloudRemote.includes(":")?a.cloudRemote.slice(a.cloudRemote.indexOf(":")+1):"";
 useEffect(()=>{let disposed=false;onReady(connected);if(unsupported||!a.cloudConnection)return;onPending(true);
  void call<Connection>({action:"cloudConnectionStatus",id:a.cloudConnection}).then(s=>{if(!disposed){setConnected(s.connected);onReady(s.connected);}}).catch(e=>{if(!disposed){setError(errorCode(e));setConnected(false);onReady(false);}}).finally(()=>{if(!disposed)onPending(false);});
  return()=>{disposed=true;};
 // Connection changes remount this step through navigation; status is checked once.
 // eslint-disable-next-line react-hooks/exhaustive-deps
 },[]);
 async function connect(){cancelled.current=false;setPending(true);onPending(true);setError("");setConnected(false);onReady(false);
  try{const prepared=await call<Connection>({action:"prepareCloud",provider});authorization.current=prepared.connectionId;
   if(cancelled.current){await call({action:"cancelCloudAuthorization",id:prepared.connectionId});throw {code:"cancelled"};}
   const result=await call<Connection>({action:"authorizeCloud",id:prepared.connectionId});
   onChange(d=>({...d,advanced:{...d.advanced,cloudConnection:result.connectionId,cloudProvider:result.provider,rcloneConfig:result.config,rcloneExecutable:result.executable,cloudRemote:`backup:${path||`NAS Backup Manager/${d.name.replace(/[\\/:\r\n]/g,"_")||"Backup"}`}`}}));
   setConnected(true);onReady(true);
  }catch(e){setError(errorCode(e));}finally{authorization.current=null;setPending(false);onPending(false);}
 }
 async function cancel(){cancelled.current=true;if(authorization.current)try{await call({action:"cancelCloudAuthorization",id:authorization.current});}catch(e){setError(errorCode(e));}}
 return <>
  <Field><FieldLabel htmlFor="cloud-provider">{t.cloudProvider}</FieldLabel><select id="cloud-provider" value={provider} disabled={pending} onChange={e=>{setProvider(e.target.value);setConnected(false);onReady(false);onChange(d=>({...d,advanced:{...d.advanced,cloudProvider:e.target.value,cloudConnection:"",rcloneConfig:""}}));}}>{a.rcloneConfig&&!a.cloudConnection&&<option value="legacy">{t.cloudExisting}</option>}<option value="onedrive">OneDrive</option><option value="dropbox">Dropbox</option></select></Field>

  <div className="path-input"><Button type="button" variant="outline" disabled={pending||provider==="legacy"} onClick={()=>void connect()}>{pending?t.cloudAuthorizing:connected?t.cloudReconnect:t.cloudConnect}</Button>{pending&&<Button type="button" variant="outline" onClick={()=>void cancel()}>{t.cancel}</Button>}</div>
  <FieldDescription>{t.cloudAutomaticHelp}</FieldDescription>
  {connected&&<p role="status" className="success">{t.cloudConnected}</p>}
  {pending&&<p role="status">{t.cloudBrowserHelp}</p>}
  <Field><FieldLabel htmlFor="cloud-folder">{t.cloudFolder}</FieldLabel><Input id="cloud-folder" value={path} required disabled={pending} onChange={e=>onChange(d=>({...d,advanced:{...d.advanced,cloudRemote:`${d.advanced.cloudRemote.split(":")[0]||"backup"}:${e.target.value}`}}))}/><FieldDescription>{t.cloudFolderHelp}</FieldDescription></Field>
  <FieldDescription>{t.cloudCacheHelp}</FieldDescription>
  {error&&<p role="alert" className="error">{t.errors[error as keyof typeof t.errors]??t.errors.filesystem}</p>}
 </>;
}
