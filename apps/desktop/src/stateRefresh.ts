// Observe only local metadata while the interface is visible. Never overlap checks.
export function pollVisible(options:{poll:()=>Promise<void>;isVisible:()=>boolean;onError:(error:unknown)=>void;delay?:number}) {
 let stopped=false,running=false,pending=false;
 let timer:ReturnType<typeof setTimeout>|undefined;
 const wake=()=>{
  clearTimeout(timer);
  if(stopped||!options.isVisible())return;
  if(running){pending=true;return;}
  running=true;
  void options.poll().catch(error=>{if(!stopped)options.onError(error);}).finally(()=>{
   running=false;
   if(!stopped&&options.isVisible())timer=setTimeout(wake,pending?0:options.delay??1000);
   pending=false;
  });
 };
 wake();
 return {wake,stop:()=>{stopped=true;clearTimeout(timer);}};
}

export function observeState(options:{changed:()=>Promise<boolean>;refresh:()=>Promise<void>;isActive:()=>boolean;onError:(error:unknown)=>void;delay?:number}) {
 let stopped=false,running=false,pending=false,needsRefresh=false;
 let timer:ReturnType<typeof setTimeout>|undefined;
 const wake=()=>{
  clearTimeout(timer);
  if(stopped)return;
  if(running){pending=true;return;}
  if(!options.isActive())return;
  running=true;
  void options.changed().then(async changed=>{
   needsRefresh ||= changed;
   if(needsRefresh&&!stopped){await options.refresh();needsRefresh=false;}
  }).catch(error=>{if(!stopped)options.onError(error);}).finally(()=>{
   running=false;
   if(stopped)return;
   if(pending){pending=false;wake();}
   else if(options.isActive())timer=setTimeout(wake,options.delay??5000);
  });
 };
 wake();
 return {wake,stop:()=>{stopped=true;clearTimeout(timer);}};
}
