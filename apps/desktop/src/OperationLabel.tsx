import type { Translations } from "./JobForm";

export function OperationLabel({kind,permanent=false,t}:{kind:string;permanent?:boolean;t:Translations}) {
 const operation=permanent&&kind==="archive_remove"?"permanent_remove":kind;
 const tone=["archive_remove","permanent_remove","deleted"].includes(operation)?"delete":["update","replaced"].includes(operation)?"update":["create","mkdir"].includes(operation)?"create":"neutral";
 const text=operation==="permanent_remove"?t.permanentRemove:t.operationKinds[operation as keyof typeof t.operationKinds]??operation;
 return <span className="operation-label" data-operation={operation} data-tone={tone}>{text}</span>;
}
