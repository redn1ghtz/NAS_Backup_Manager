import { useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import {call,errorCode,type JobDraft,type AdvancedOptions,type JobSecrets} from "./api";
import type { Translations } from "./JobForm";

export default function AdvancedSettings({draft,onChange,t}:{draft:JobDraft;onChange:React.Dispatch<React.SetStateAction<JobDraft>>;t:Translations}) {
 const a=draft.advanced;const [error,setError]=useState("");
 const [generating,setGenerating]=useState(false);const [keyPath,setKeyPath]=useState("");
 const set=<K extends keyof AdvancedOptions>(key:K,value:AdvancedOptions[K])=>onChange(d=>({...d,advanced:{...d.advanced,[key]:value}}));
 const secret=(key:keyof JobSecrets,value:string)=>onChange(d=>({...d,secrets:{...d.secrets,[key]:value}}));

 const text=(key:keyof AdvancedOptions,label:string)=> <Field><FieldLabel htmlFor={`advanced-${key}`}>{label}</FieldLabel><Input id={`advanced-${key}`} autoComplete="off" value={String(a[key])} onChange={e=>set(key,e.target.value as never)}/></Field>;
 const number=(key:keyof AdvancedOptions,label:string,min=0,max=4294967295)=><Field><FieldLabel htmlFor={`advanced-${key}`}>{label}</FieldLabel><Input id={`advanced-${key}`} type="number" min={min} max={max} step={1} required value={Number(a[key])} onChange={e=>set(key,Number(e.target.value) as never)}/></Field>;
 const toggle=(key:keyof AdvancedOptions,label:string)=><label className="consent"><input type="checkbox" checked={Boolean(a[key])} onChange={e=>set(key,e.target.checked as never)}/>{label}</label>;
 async function importIdentity(){const path=await open({multiple:false,directory:false,title:t.ageIdentity});if(typeof path==="string"){const content=await call<string>({action:"readIdentity",path});secret("ageIdentity",content);}}
 async function generateIdentity(){setGenerating(true);setError("");try{const path=await save({title:t.ageSaveRecovery,defaultPath:"backup-recovery.agekey",filters:[{name:t.ageIdentity,extensions:["agekey"]}]});if(!path)return;const result=await call<{recipient:string;identity:string}>({action:"generateIdentity",ageExecutable:a.ageExecutable,path});onChange(d=>({...d,advanced:{...d.advanced,encryptionRecipient:result.recipient},secrets:{...d.secrets,ageIdentity:result.identity}}));setKeyPath(path);}catch(e){setError(errorCode(e));}finally{setGenerating(false);}}
 return <details className="advanced-settings"><summary>{t.advancedTitle}</summary><p>{t.advancedHelp}</p>
  <details><summary>{t.integrityTitle}</summary>{toggle("verifyEnabled",t.integrityEnable)}{a.verifyEnabled&&<>{number("verifyEveryDays",t.integrityEvery,1)}</>}{number("verifyMibPerSecond",t.integritySpeed,0)}<FieldDescription>{t.integrityHelp}</FieldDescription></details>
  <details><summary>{t.retryTitle}</summary>{number("retryAttempts",t.retryAttempts,0,20)}{number("retryInitialSeconds",t.retryInitial,1,3600)}{number("retryMaxSeconds",t.retryMax,1,3600)}<FieldDescription>{t.retryHelp}</FieldDescription></details>
  <details><summary>{t.bandwidthTitle}</summary>{number("bandwidthKib",t.bandwidthLimit)}{a.bandwidthKib>0&&<div className="advanced-pair">{(["bandwidthStartMinute","bandwidthEndMinute"] as const).map((key,i)=><Field key={key}><FieldLabel htmlFor={key}>{i?t.bandwidthEnd:t.bandwidthStart}</FieldLabel><Input id={key} type="time" required value={`${String(Math.floor(a[key]/60)).padStart(2,"0")}:${String(a[key]%60).padStart(2,"0")}`} onChange={e=>{const [h,m]=e.target.value.split(":").map(Number);if(Number.isFinite(h)&&Number.isFinite(m))set(key,h*60+m);}}/></Field>)}</div>}<FieldDescription>{t.bandwidthHelp}</FieldDescription></details>
  <details><summary>{t.ageTitle}</summary><Button type="button" variant="outline" disabled={generating||!a.ageExecutable||Boolean(a.encryptionRecipient)} onClick={()=>void generateIdentity()}>{generating?t.loading:t.ageGenerate}</Button><FieldDescription>{t.ageGenerateHelp}</FieldDescription>{keyPath&&<p role="status" className="message">{t.ageRecoverySaved} {keyPath}</p>}{text("encryptionRecipient",t.ageRecipient)}{a.encryptionRecipient&&<>{text("ageExecutable",t.ageExecutable)}<Button type="button" variant="outline" disabled={generating} onClick={()=>void importIdentity().catch(e=>setError(errorCode(e)))}>{t.ageImport}</Button><p role="status">{draft.secrets?.ageIdentity?t.ageImported:t.ageIdentityHelp}</p></>}<FieldDescription>{t.ageHelp}</FieldDescription></details>

  <details><summary>{t.externalNotifications}</summary>{toggle("telegramEnabled",t.telegramEnable)}{toggle("smtpEnabled",t.smtpEnable)}<FieldDescription>{t.notificationTaskHelp}</FieldDescription></details>
  {error&&<p className="error" role="alert">{t.errors[error as keyof typeof t.errors]??t.errors.filesystem}</p>}<p className="message">{t.vaultHelp}</p>
 </details>;
}
