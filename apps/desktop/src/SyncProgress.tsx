import { RefreshCw, Square } from "lucide-react";
import { Button } from "@/components/ui/button";
import {size,progressPercent,remainingLabel,type LiveProgress} from "./api";
import type {Translations} from "./JobForm";
export function SyncProgress({activity,t,onCancel,pending}:{activity:LiveProgress;t:Translations;onCancel:(activity:LiveProgress)=>void;pending:boolean}) {
 const scanning=activity.phase==="scanning";
 const percent=progressPercent(activity);
 const eta=remainingLabel(Date.now()/1000-activity.updatedAt>5?null:activity.etaSeconds,t);
 const cancelling=pending||activity.cancelRequested;
 const phase=cancelling?t.cancelling:t.progressPhases[activity.phase as keyof typeof t.progressPhases]??t.running;
 return <section className="sync-progress" aria-label={`${t.syncProgress}: ${activity.name}`}>
  <div className="progress-heading"><strong><RefreshCw aria-hidden="true"/>{activity.name}</strong><div className="progress-controls"><span><span role="status">{phase}</span>{percent!==undefined?` · ${percent}%`:""}</span><Button variant="outline" size="sm" disabled={!activity.executionId||cancelling||activity.phase==="finalizing"} title={t.cancelSyncHelp} aria-label={`${t.cancelSync}: ${activity.name}`} onClick={()=>onCancel(activity)}><Square data-icon="inline-start" aria-hidden="true"/>{cancelling?t.cancelling:t.cancelSync}</Button></div></div>
  <progress title={t.progressWorkHint} max={100} value={percent} aria-label={`${activity.name}: ${phase}`}/>
  <div className="progress-details">{!scanning&&<p className="progress-counts">{activity.completed} / {activity.total} · {size(activity.copiedBytes,t.locale)} / {size(activity.totalBytes,t.locale)}</p>}
  <p className="progress-eta">{eta}</p>{activity.currentPath&&<p className="job-path" title={activity.currentPath}>{activity.currentPath}</p>}</div>
 </section>;
}
