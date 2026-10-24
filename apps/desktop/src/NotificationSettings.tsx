import { useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldLabel, FieldDescription } from "@/components/ui/field";
import { call, errorCode } from "./api";
import type { Translations } from "./JobForm";
interface Fields {telegramChat:string;smtpUrl:string;smtpUsername:string;smtpFrom:string;smtpTo:string;curlExecutable:string}
interface View {fields:Fields;telegramConfigured:boolean;smtpConfigured:boolean;legacyJobs:{id:string;name:string}[];legacyMore:boolean}
export default function NotificationSettings({t,disabled}:{t:Translations;disabled:boolean}) {
 const [data,setData]=useState<View>();const [fields,setFields]=useState<Fields>();
 const [telegramToken,setTelegramToken]=useState("");const [smtpPassword,setSmtpPassword]=useState("");
 const [pending,setPending]=useState(false);const [error,setError]=useState("");const [saved,setSaved]=useState(false);const [legacy,setLegacy]=useState("");const [offset,setOffset]=useState(0);
 const accept=(value:View)=>{setData(value);setFields(value.fields);setTelegramToken("");setSmtpPassword("");setOffset(0);};
 useEffect(()=>{let disposed=false;void call<View>({action:"notificationSettings"}).then(value=>{if(!disposed)accept(value);}).catch(e=>{if(!disposed)setError(errorCode(e));});return()=>{disposed=true;};},[]);
 async function perform(action:()=>Promise<View>){setPending(true);setError("");setSaved(false);try{accept(await action());setSaved(true);}catch(e){setError(errorCode(e));}finally{setPending(false);}}
 async function more(){setPending(true);try{const next=await call<View>({action:"notificationSettings",offset:offset+50});setOffset(offset+50);setData(old=>old?{...old,legacyJobs:[...old.legacyJobs,...next.legacyJobs],legacyMore:next.legacyMore}:next);}catch(e){setError(errorCode(e));}finally{setPending(false);}}
 const blocked=disabled||pending||!fields;
 const text=(key:keyof Fields,label:string,type="text")=><Field><FieldLabel htmlFor={`notification-${key}`}>{label}</FieldLabel><Input id={`notification-${key}`} type={type} autoComplete="off" disabled={blocked} value={fields?.[key]??""} onChange={e=>{setFields(f=>f?{...f,[key]:e.target.value}:f);setSaved(false);}}/></Field>;
 return <div className="global-notification-settings"><h2>{t.externalNotifications}</h2><p>{t.notificationSettingsHelp}</p>
  {!fields&&!error&&<p role="status">{t.loading}</p>}
  <form onSubmit={e=>{e.preventDefault();if(fields)void perform(()=>call<View>({action:"saveNotificationSettings",update:{fields,telegramToken,smtpPassword}}));}}>
   <details><summary>Telegram {data?.telegramConfigured&&`· ${t.notificationConfigured}`}</summary>
    {text("telegramChat",t.telegramChat)}<Field><FieldLabel htmlFor="notification-telegramToken">{t.telegramToken}</FieldLabel><Input id="notification-telegramToken" type="password" autoComplete="new-password" spellCheck={false} disabled={blocked} value={telegramToken} onChange={e=>{setTelegramToken(e.target.value);setSaved(false);}}/><FieldDescription>{t.secretKeep}</FieldDescription></Field>
   </details>
   <details><summary>{t.smtpSettings} {data?.smtpConfigured&&`· ${t.notificationConfigured}`}</summary>
    {text("smtpUrl",t.smtpUrl)}{text("smtpUsername",t.smtpUsername)}{text("smtpFrom",t.smtpFrom,"email")}{text("smtpTo",t.smtpTo,"email")}
    <Field><FieldLabel htmlFor="notification-smtpPassword">{t.smtpPassword}</FieldLabel><Input id="notification-smtpPassword" type="password" autoComplete="new-password" spellCheck={false} disabled={blocked} value={smtpPassword} onChange={e=>{setSmtpPassword(e.target.value);setSaved(false);}}/><FieldDescription>{t.secretKeep}</FieldDescription></Field>
   </details>
   <p>{t.notificationProtectionHelp}</p><Button disabled={blocked} type="submit">{pending?t.loading:t.saveNotificationSettings}</Button>
  </form>
  {!!data?.legacyJobs.length&&<details className="legacy-notification-settings"><summary>{t.notificationLegacy}</summary><p>{t.notificationLegacyHelp}</p><label className="selection-label">{t.chooseJob}<select disabled={blocked} value={legacy} onChange={e=>setLegacy(e.target.value)}><option value="">—</option>{data.legacyJobs.map(job=><option key={job.id} value={job.id}>{job.name}</option>)}</select></label><Button disabled={blocked||!legacy} variant="outline" onClick={()=>void perform(()=>call<View>({action:"importNotificationSettings",id:legacy}))}>{t.notificationImport}</Button>{data.legacyMore&&<Button disabled={blocked} variant="ghost" onClick={()=>void more()}>{t.notificationMore}</Button>}</details>}
  {saved&&<p className="success" role="status">{t.notificationSaved}</p>}{error&&<p className="error" role="alert">{t.errors[error as keyof typeof t.errors]??t.errors.filesystem}</p>}
 </div>;
}
