import { AlertCircle, CheckCircle2, Clock3, Info, PauseCircle } from "lucide-react";
import {isSourceChanged} from "./api";
import type { Translations } from "./JobForm";

export function StatusBadge({state,kind,errorCode,t}: {state:string|null;kind?:string;errorCode?:string|null;t:Translations}) {
  let Icon=Info;let tone="muted";let text=t.never;
  if(kind){
    if(state==="committed"){
      if(kind==="unchanged"){text=t.operationKinds.unchanged;}
      else if(kind==="archive_remove"){text=t.archived;tone="info";}
      else {Icon=CheckCircle2;tone="success";text=kind==="permanent_remove"?t.removed:t.operationKinds.committed;}
    }else if(state==="failed"){Icon=AlertCircle;tone="error";text=t.states.failed;}
    else {text=t.notCompleted;tone="warning";}
  }else if(state){
    text=t.states[state as keyof typeof t.states]??t.operationKinds[state as keyof typeof t.operationKinds]??state;
    if(state==="succeeded"){Icon=CheckCircle2;tone="success";}
    else if(state==="failed"||state==="interrupted"){Icon=AlertCircle;tone="error";}
    else if(["running","scanning","planned"].includes(state)){Icon=Clock3;tone="info";}
    else if(state==="paused"){Icon=PauseCircle;text=t.pausedLabel;}
  }
  if(!kind&&isSourceChanged(errorCode)){Icon=Info;tone="info";text=t.needsSync;}
  if(!kind&&errorCode==="cancelled"){Icon=PauseCircle;tone="muted";text=t.cancelled;}
  return <span className={`status-badge status-${tone}`}><Icon aria-hidden="true"/><span>{text}</span></span>;
}
