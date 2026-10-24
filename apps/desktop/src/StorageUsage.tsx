import { size } from "./api";
import type { Translations } from "./JobForm";

export interface StorageValues {total:number|null;used:number|null;free:number|null}
export function StorageUsage({values,t,language}:{values:StorageValues;t:Translations;language:string}) {
 const {total,used,free}=values;
 const percent=total!==null&&total>0&&used!==null?Math.max(0,Math.min(100,Math.round(used/total*100))):undefined;
 return <>
  <div className="storage-details"><svg className="storage-ring" viewBox="0 0 120 120" role="img" aria-label={`${t.used}: ${percent===undefined?"—":`${percent}%`}`}><circle cx="60" cy="60" r="48" className="ring-track"/>{percent!==undefined&&<circle cx="60" cy="60" r="48" pathLength="100" strokeDasharray={`${percent} 100`} className="ring-value"/>}<text x="60" y="65" textAnchor="middle">{percent===undefined?"—":`${percent}%`}</text></svg><dl>{(["used","free","total"] as const).map(key=><div key={key}><dt>{t[key]}</dt><dd>{values[key]===null?"—":size(values[key]!,language)}</dd></div>)}</dl></div>
  <progress max={100} value={percent??0} aria-label={t.used} aria-valuetext={percent===undefined?t.cloudQuotaUnknown:`${percent}%`}/>
 </>;
}
