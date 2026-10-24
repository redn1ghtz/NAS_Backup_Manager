import {useState} from "react";
import {open} from "@tauri-apps/plugin-dialog";
import {Button} from "@/components/ui/button";
import {Input} from "@/components/ui/input";
import {Field,FieldGroup,FieldLabel} from "@/components/ui/field";
import {Dialog,DialogContent,DialogHeader,DialogTitle,DialogDescription,DialogFooter} from "@/components/ui/dialog";
import {call,errorCode,type Job} from "./api";
import type {Translations} from "./JobForm";
export default function DecryptDialog({job,t,onClose}:{job:Job;t:Translations;onClose:()=>void}){
 const [relative,setRelative]=useState("");const [directory,setDirectory]=useState("");const [pending,setPending]=useState(false);const [error,setError]=useState("");const [done,setDone]=useState(false);
 async function submit(){setPending(true);setError("");try{await call({action:"decryptFile",id:job.id,relative,directory});setDone(true);}catch(e){setError(errorCode(e));}finally{setPending(false);}}
 return <Dialog open onOpenChange={v=>{if(!v&&!pending)onClose();}}><DialogContent showCloseButton={false}><DialogHeader><DialogTitle>{t.decryptFile}</DialogTitle><DialogDescription>{t.ageHelp}</DialogDescription></DialogHeader><form onSubmit={e=>{e.preventDefault();void submit();}}><FieldGroup><Field><FieldLabel htmlFor="encrypted-relative">{t.relativeEncryptedPath}</FieldLabel><Input id="encrypted-relative" required value={relative} onChange={e=>{setRelative(e.target.value);setDone(false);}}/></Field><Field><FieldLabel htmlFor="decrypt-destination">{t.destination}</FieldLabel><div className="path-input"><Input id="decrypt-destination" required value={directory} onChange={e=>setDirectory(e.target.value)}/><Button type="button" variant="outline" onClick={()=>void open({directory:true,multiple:false}).then(p=>{if(typeof p==="string")setDirectory(p);}).catch(e=>setError(errorCode(e)))}>{t.browse}</Button></div></Field></FieldGroup>{error&&<p role="alert" className="error">{t.errors[error as keyof typeof t.errors]??t.errors.filesystem}</p>}{done&&<p role="status" className="success">{t.complete}</p>}<DialogFooter className="form-footer"><Button type="button" variant="outline" disabled={pending} onClick={onClose}>{t.close}</Button><Button disabled={pending||!relative||!directory}>{pending?t.loading:t.decryptFile}</Button></DialogFooter></form></DialogContent></Dialog>;
}
