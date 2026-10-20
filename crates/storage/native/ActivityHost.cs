using System;
using System.IO;
using System.Text;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;
using System.Threading;
using System.Collections.Generic;
using System.Collections.Concurrent;
using System.Web.Script.Serialization;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.ComTypes;
using Microsoft.Win32;

// Built with the Windows .NET Framework compiler. No resident service or PowerShell host.
static class ActivityHost {
 const string AppId="org.nasbackupmanager.desktop";
 const string ToastClsid="12D494FA-263E-4981-92DF-19C7059FFAD2";
 static readonly JavaScriptSerializer Json=new JavaScriptSerializer();
 static readonly ConcurrentQueue<string> Commands=new ConcurrentQueue<string>();
 static TextReader Input;static TextWriter Output;
 static Type ManagerType,XmlType,ToastType;
 static string ApplicationPath,Job,Language;
 static bool Notifications;
 static object Notifier;
 static int NotificationCount;
 static int HistoryCount=-1;
 static string NotificationError;
 static bool Russian {get{return Language=="ru";}}
 static void Report(string stage){Output.WriteLine(Json.Serialize(new {stage=stage,notifications=NotificationCount,history=HistoryCount,job=Job,error=NotificationError}));Output.Flush();}
 static string Escape(string value){return System.Security.SecurityElement.Escape(value)??"";}
 static void Toast(string title,string body){
  if(!Notifications)return;
  string stage="identity";
  try{
   if(Notifier==null){
    InstallIdentity();
    ManagerType=Type.GetType("Windows.UI.Notifications.ToastNotificationManager, Windows, ContentType=WindowsRuntime",true);
    XmlType=Type.GetType("Windows.Data.Xml.Dom.XmlDocument, Windows, ContentType=WindowsRuntime",true);
    ToastType=Type.GetType("Windows.UI.Notifications.ToastNotification, Windows, ContentType=WindowsRuntime",true);
    Notifier=ManagerType.GetMethod("CreateToastNotifier",new Type[]{typeof(string)}).Invoke(null,new object[]{AppId});
   }
   stage="xml";object xml=Activator.CreateInstance(XmlType);
   XmlType.GetMethod("LoadXml",new Type[]{typeof(string)}).Invoke(xml,new object[]{"<toast activationType='protocol' launch='nasbackupmanager://open'><visual><binding template='ToastGeneric'><text>"+Escape(title)+"</text><text>"+Escape(body)+"</text></binding></visual><audio silent='true'/></toast>"});
   stage="toast";object toast=Activator.CreateInstance(ToastType,new object[]{xml});
   stage="show";Notifier.GetType().GetMethod("Show",new Type[]{ToastType}).Invoke(Notifier,new object[]{toast});
   NotificationCount++;
   try{var history=ManagerType.GetProperty("History").GetValue(null,null);var items=(System.Collections.IEnumerable)history.GetType().GetMethod("GetHistory",new Type[]{typeof(string)}).Invoke(history,new object[]{AppId});int count=0;foreach(var item in items)count++;HistoryCount=count;}catch{}
  }catch(Exception e){var cause=e.InnerException??e;NotificationError=stage+":"+cause.GetType().Name+":"+cause.HResult.ToString("X8");}
 }
 static void InstallIdentity(){
  if(!Path.IsPathRooted(ApplicationPath)||!File.Exists(ApplicationPath))throw new InvalidOperationException("application_missing");
  using(var key=Registry.CurrentUser.CreateSubKey("Software\\Classes\\AppUserModelId\\"+AppId)){
   SetValueIfChanged(key,"DisplayName","NAS Backup Manager");SetValueIfChanged(key,"IconUri",ApplicationPath);
  }
  using(var priorProtocol=Registry.CurrentUser.OpenSubKey("Software\\Classes\\nasbackupmanager")){
   if(priorProtocol!=null && Convert.ToString(priorProtocol.GetValue("Owner"))!=AppId)throw new InvalidOperationException("protocol_conflict");
  }
  using(var protocol=Registry.CurrentUser.CreateSubKey("Software\\Classes\\nasbackupmanager")){
   SetValueIfChanged(protocol,"","URL:NAS Backup Manager");SetValueIfChanged(protocol,"URL Protocol","");SetValueIfChanged(protocol,"Owner",AppId);
   using(var command=protocol.CreateSubKey("shell\\open\\command")){SetValueIfChanged(command,"","\""+ApplicationPath+"\" --show");}
  }
  string shortcut=Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs),"NAS Backup Manager.lnk");
  object link=new ShellLink();
  try{
   if(File.Exists(shortcut)){
    ((IPersistFile)link).Load(shortcut,2);
    PropVariant prior;var appKey=new PropertyKey(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"),5);
    ((IPropertyStore)link).GetValue(ref appKey,out prior);
    string previous=prior.Type==31?Marshal.PtrToStringUni(prior.Pointer):null;
    PropVariantClear(ref prior);
    if(previous!=AppId)throw new InvalidOperationException("shortcut_conflict");
    var target=new StringBuilder(32768);var arguments=new StringBuilder(32768);
    ((IShellLink)link).GetPath(target,target.Capacity,IntPtr.Zero,4);
    ((IShellLink)link).GetArguments(arguments,arguments.Capacity);
    var activatorKey=new PropertyKey(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"),26);
    PropVariant registered;((IPropertyStore)link).GetValue(ref activatorKey,out registered);
    bool sameClsid=false;
    try{sameClsid=registered.Type==72&&registered.Pointer!=IntPtr.Zero&&(Guid)Marshal.PtrToStructure(registered.Pointer,typeof(Guid))==new Guid(ToastClsid);}finally{PropVariantClear(ref registered);}
    // Registration is setup, not an operation to repeat for each backup. Avoid
    // rewriting the Start Menu link/registry on every start/end notification.
    if(String.Equals(target.ToString(),ApplicationPath,StringComparison.OrdinalIgnoreCase)&&arguments.Length==0&&sameClsid)return;
   }
   ((IShellLink)link).SetPath(ApplicationPath);
   ((IShellLink)link).SetArguments("");
   ((IShellLink)link).SetDescription("NAS Backup Manager");
   ((IShellLink)link).SetIconLocation(ApplicationPath,0);
   var property=new PropertyKey(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"),5);
   var value=new PropVariant{Type=31,Pointer=Marshal.StringToCoTaskMemUni(AppId)};
   try{((IPropertyStore)link).SetValue(ref property,ref value);((IPropertyStore)link).Commit();}
   finally{PropVariantClear(ref value);}
   var activator=new PropertyKey(new Guid("9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3"),26);
   var clsid=new PropVariant{Type=72,Pointer=Marshal.AllocCoTaskMem(16)};
   try{Marshal.Copy(new Guid(ToastClsid).ToByteArray(),0,clsid.Pointer,16);((IPropertyStore)link).SetValue(ref activator,ref clsid);((IPropertyStore)link).Commit();}finally{PropVariantClear(ref clsid);}
   ((IPersistFile)link).Save(shortcut,true);
  }finally{Marshal.FinalReleaseComObject(link);}
 }
 static void SetValueIfChanged(RegistryKey key,string name,string value){
  if(!String.Equals(key.GetValue(name) as string,value,StringComparison.Ordinal))key.SetValue(name,value,RegistryValueKind.String);
 }
 [STAThread]
 static void Main(string[] args){
  try{
   Input=new StreamReader(Console.OpenStandardInput(),new UTF8Encoding(false));Output=new StreamWriter(Console.OpenStandardOutput(),new UTF8Encoding(false)){AutoFlush=true};
   if(args.Length==2 && args[0]=="--register"){ApplicationPath=args[1];InstallIdentity();return;}
   if(args.Length==2 && args[0]=="--open"){ApplicationPath=args[1];OpenInterface();return;}
   if(args.Length==2 && args[0]=="--focus"){Environment.ExitCode=FocusExisting(args[1])?0:2;return;}
   var first=Json.Deserialize<Dictionary<string,object>>(Input.ReadLine()??"{}");
   ApplicationPath=Convert.ToString(first["application"]);Job=Convert.ToString(first["job"]);Language=Convert.ToString(first["language"]);
   Notifications=Convert.ToBoolean(first["notifications"]);
   Process owner=Process.GetProcessById(Convert.ToInt32(first["owner"]));
   Application.EnableVisualStyles();
   using(var icon=new NotifyIcon())using(var menu=new ContextMenuStrip())using(var timer=new System.Windows.Forms.Timer()){
    Icon ownedIcon=null;
    try{ownedIcon=Icon.ExtractAssociatedIcon(ApplicationPath);}catch{}
    icon.Icon=ownedIcon??SystemIcons.Information;
    string tip=(Russian?"Синхронизация: ":"Synchronizing: ")+Job;
    icon.Text=tip.Length>63?tip.Substring(0,63):tip;
    var open=menu.Items.Add(Russian?"Открыть NAS Backup Manager":"Open NAS Backup Manager");
    EventHandler launch=delegate{OpenInterface();};
    open.Click+=launch;icon.MouseClick+=delegate(object sender,MouseEventArgs e){if(e.Button==MouseButtons.Left)OpenInterface();};icon.ContextMenuStrip=menu;icon.Visible=true;
    Toast(Russian?"Синхронизация началась":"Synchronization started",Job);
    Report("started");
    var context=new ApplicationContext();bool errorIcon=false;
    menu.Items.Add(Russian?"Скрыть значок":"Dismiss icon",null,delegate{context.ExitThread();});
    var reader=new Thread(delegate(){try{string line;while((line=Input.ReadLine())!=null)Commands.Enqueue(line);}finally{Commands.Enqueue("{\"action\":\"close\"}");}});
    reader.IsBackground=true;reader.Start();
    timer.Interval=250;
    timer.Tick+=delegate{
     if(errorIcon){context.ExitThread();return;}
     try{if(owner.HasExited){context.ExitThread();return;}}catch{context.ExitThread();return;}
     string line;
     while(Commands.TryDequeue(out line)){
      try{
       var command=Json.Deserialize<Dictionary<string,object>>(line);
       string action=Convert.ToString(command["action"]);
       if(action=="notifications"){Notifications=Convert.ToBoolean(command["enabled"]);}
       else if(action=="progress"){
        string text=tip+" · "+Convert.ToString(command["percent"])+"%";icon.Text=text.Length>63?text.Substring(0,63):text;
       }else if(action=="finish"){
        string result=Convert.ToString(command["result"]);
        string title=result=="succeeded"?(Russian?"Синхронизация завершена":"Synchronization completed"):result=="cancelled"?(Russian?"Синхронизация отменена":"Synchronization cancelled"):result=="changed"?(Russian?"Файлы изменились":"Files changed"):(Russian?"Синхронизация не завершена":"Synchronization did not complete");
        string body=Job+(result=="changed"?(Russian?". Файлы будут обновлены при следующей синхронизации.":". Files will be updated on the next synchronization."):"");
        if(result=="failed")body+=Russian?". Откройте журнал приложения для подробностей.":". Open the application log for details.";
        Toast(title,body);
        if(result=="failed"){
         errorIcon=true;icon.Icon=SystemIcons.Error;string failure=(Russian?"Ошибка синхронизации: ":"Synchronization error: ")+Job;icon.Text=failure.Length>63?failure.Substring(0,63):failure;
         timer.Interval=30000;Report("finished_error");
        }else{Report("finished");context.ExitThread();}
       }else if(action=="close"&&!errorIcon){context.ExitThread();}
      }catch{NotificationError="invalid_message";}
     }
    };
    timer.Start();Application.Run(context);timer.Stop();icon.Visible=false;
    if(ownedIcon!=null)ownedIcon.Dispose();context.Dispose();
   }
   owner.Dispose();
  }catch(Exception e){NotificationError=e.GetType().Name;Report("failed");Environment.ExitCode=1;}
 }
 static void OpenInterface(){try{if(!FocusExisting(ApplicationPath))Process.Start(new ProcessStartInfo(ApplicationPath,"--show"){UseShellExecute=true});}catch{}}
 static bool FocusExisting(string application){
  foreach(var candidate in Process.GetProcessesByName(Path.GetFileNameWithoutExtension(application))){
   using(candidate){try{
    if(!String.Equals(Path.GetFullPath(candidate.MainModule.FileName),Path.GetFullPath(application),StringComparison.OrdinalIgnoreCase))continue;
    // MainWindowHandle can select an untitled auxiliary window when the real
    // GUI is hidden. Always identify the owned application window by title.
    IntPtr window=IntPtr.Zero;
    EnumWindows(delegate(IntPtr handle,IntPtr ignored){
      uint pid;GetWindowThreadProcessId(handle,out pid);
      if(pid!=(uint)candidate.Id)return true;
      var title=new StringBuilder(256);GetWindowText(handle,title,title.Capacity);
      if(title.ToString()!="NAS Backup Manager")return true;
      window=handle;return false;
    },IntPtr.Zero);
    if(window==IntPtr.Zero)continue;
    ShowWindowAsync(window,IsIconic(window)?9:5);SetForegroundWindow(window);return true;
   }catch{}}
  }
  return false;
 }
 [DllImport("user32.dll")]static extern bool SetForegroundWindow(IntPtr window);
 [DllImport("user32.dll")]static extern bool ShowWindowAsync(IntPtr window,int command);
 [DllImport("user32.dll")]static extern bool IsIconic(IntPtr window);
 delegate bool EnumWindowCallback(IntPtr window,IntPtr parameter);
 [DllImport("user32.dll")]static extern bool EnumWindows(EnumWindowCallback callback,IntPtr parameter);
 [DllImport("user32.dll")]static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)]static extern int GetWindowText(IntPtr window,StringBuilder title,int count);
 [DllImport("ole32.dll")]static extern int PropVariantClear(ref PropVariant value);
 [StructLayout(LayoutKind.Sequential,Pack=4)]struct PropertyKey{public Guid Format;public uint Id;public PropertyKey(Guid format,uint id){Format=format;Id=id;}}
#if HOST_X86
 [StructLayout(LayoutKind.Explicit,Size=16)]
#else
 [StructLayout(LayoutKind.Explicit,Size=24)]
#endif
 struct PropVariant{[FieldOffset(0)]public ushort Type;[FieldOffset(8)]public IntPtr Pointer;}
 [ComImport,Guid("00021401-0000-0000-C000-000000000046")]class ShellLink{}
 [ComImport,Guid("886D8EEB-8CF2-4446-8D02-CDBA1DBDCF99"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]interface IPropertyStore{
  void GetCount(out uint count);void GetAt(uint index,out PropertyKey key);void GetValue(ref PropertyKey key,out PropVariant value);void SetValue(ref PropertyKey key,ref PropVariant value);void Commit();
 }
 [ComImport,Guid("000214F9-0000-0000-C000-000000000046"),InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]interface IShellLink{
  void GetPath([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder file,int count,IntPtr data,uint flags);
  void GetIDList(out IntPtr list);void SetIDList(IntPtr list);
  void GetDescription([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder text,int count);void SetDescription([MarshalAs(UnmanagedType.LPWStr)]string text);
  void GetWorkingDirectory([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder path,int count);void SetWorkingDirectory([MarshalAs(UnmanagedType.LPWStr)]string path);
  void GetArguments([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder args,int count);void SetArguments([MarshalAs(UnmanagedType.LPWStr)]string args);
  void GetHotkey(out short value);void SetHotkey(short value);void GetShowCmd(out int command);void SetShowCmd(int command);
  void GetIconLocation([Out,MarshalAs(UnmanagedType.LPWStr)]StringBuilder path,int count,out int index);void SetIconLocation([MarshalAs(UnmanagedType.LPWStr)]string path,int index);
  void SetRelativePath([MarshalAs(UnmanagedType.LPWStr)]string path,uint reserved);void Resolve(IntPtr window,uint flags);void SetPath([MarshalAs(UnmanagedType.LPWStr)]string path);
 }
}
