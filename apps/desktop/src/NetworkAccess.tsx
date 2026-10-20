import { useEffect, useState } from "react";
import { ShieldCheck } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { call,errorCode } from "./api";
import type { Translations } from "./JobForm";

export default function NetworkAccess({path,t,onPending,loadSaved=true}: {path:string;t:Translations;onPending?:(pending:boolean)=>void;loadSaved?:boolean}) {
  const [mode,setMode]=useState<"windows"|"guest"|"credentials">("windows");
  const [username,setUsername]=useState("");const [password,setPassword]=useState("");
  const [pending,setPending]=useState(false);const [saved,setSaved]=useState<"windows"|"guest"|"credentials">();const [error,setError]=useState("");
  useEffect(()=>{if(loadSaved&&path)void action("status");},[loadSaved,path]);
  async function action(kind:"status"|"save"|"forget"){
    setPending(true);onPending?.(true);setError("");
    try {
      if(kind==="status"){const value=await call<"windows"|"guest"|"credentials">({action:"networkProfile",path});setSaved(value);setMode(value);}
      if(kind==="save"){
        if(mode==="guest")await call({action:"saveGuestNetworkAccess",path});
        else if(mode==="windows")await call({action:"forgetNetworkCredentials",path});
        else await call({action:"saveNetworkCredentials",path,username,password});
        setPassword("");setSaved(mode);
      }
      if(kind==="forget"){await call({action:"forgetNetworkCredentials",path});setPassword("");setSaved("windows");setMode("windows");}
    }catch(e){const code=errorCode(e);setError(t.errors[code as keyof typeof t.errors]??t.errors.filesystem);}
    finally{setPending(false);onPending?.(false);}
  }
  return <fieldset className="network-access" disabled={pending} onKeyDown={e=>{if(e.key==="Enter"){e.preventDefault();if(!pending&&path&&(mode!=="credentials"||(username.trim()&&password)))void action("save");}}}><legend><ShieldCheck aria-hidden="true"/>{t.networkAccess}</legend><p>{t.networkAccessHelp}</p>
    <label htmlFor="network-mode">{t.networkMode}</label><select id="network-mode" value={mode} onChange={e=>{setMode(e.target.value as typeof mode);setPassword("");setError("");}}><option value="windows">{t.networkWindowsMode}</option><option value="guest">{t.networkGuestMode}</option><option value="credentials">{t.networkCredentialsMode}</option></select>
    {mode==="guest"&&<p>{t.networkGuestHelp}</p>}
    {mode==="credentials"&&<><label htmlFor="network-username">{t.networkUsername}</label><Input id="network-username" autoComplete="off" value={username} onChange={e=>setUsername(e.target.value)}/>
    <label htmlFor="network-password">{t.networkPassword}</label><Input id="network-password" type="password" autoComplete="new-password" value={password} onChange={e=>setPassword(e.target.value)}/></>}
    <div className="row-actions"><Button type="button" variant="outline" disabled={!path||(mode==="credentials"&&(!username.trim()||!password))} onClick={()=>void action("save")}>{mode==="credentials"?t.saveNetworkAccess:t.saveNetworkMode}</Button><Button type="button" variant="ghost" disabled={!path} onClick={()=>void action("status")}>{t.networkCredentialStatus}</Button>{saved&&saved!=="windows"&&<Button type="button" variant="ghost" onClick={()=>void action("forget")}>{t.forgetNetworkAccess}</Button>}</div>
    {pending&&<p role="status">{t.loading}</p>}{saved!==undefined&&<p role="status" className="success">{saved==="guest"?t.networkGuestSaved:saved==="credentials"?t.networkAccessSaved:t.networkWindowsSaved}</p>}{error&&<p role="alert" className="error">{error}</p>}
  </fieldset>;
}
