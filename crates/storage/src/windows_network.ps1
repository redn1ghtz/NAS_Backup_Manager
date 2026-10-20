$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::InputEncoding = New-Object System.Text.UTF8Encoding($false)
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
try {
    # Deserialize in managed code: no secret JSON enters cmdlet parameter logging.
    Add-Type -AssemblyName System.Web.Extensions
    $serializer = New-Object System.Web.Script.Serialization.JavaScriptSerializer
    $request = $serializer.DeserializeObject([Console]::In.ReadToEnd())
    if ($request.action -eq 'connect' -and [System.IO.Directory]::Exists([string]$request.path)) {
        [Console]::WriteLine('0')
        exit 0
    }
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class NasBackupNetwork {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    public struct Credential {
        public uint Flags, Type;
        public string TargetName, Comment;
        public long LastWritten;
        public uint CredentialBlobSize;
        public IntPtr CredentialBlob;
        public uint Persist, AttributeCount;
        public IntPtr Attributes;
        public string TargetAlias, UserName;
    }
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    public struct Resource {
        public uint Scope, Type, DisplayType, Usage;
        public string LocalName, RemoteName, Comment, Provider;
    }
    [DllImport("advapi32.dll", EntryPoint="CredWriteW", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool Write(ref Credential credential, uint flags);
    [DllImport("advapi32.dll", EntryPoint="CredReadW", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool Read(string target, uint type, uint flags, out IntPtr credential);
    [DllImport("advapi32.dll", EntryPoint="CredDeleteW", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern bool Delete(string target, uint type, uint flags);
    [DllImport("advapi32.dll")] static extern void CredFree(IntPtr credential);
    [DllImport("mpr.dll", EntryPoint="WNetAddConnection2W", CharSet=CharSet.Unicode)]
    static extern uint Add(ref Resource resource, string password, string username, uint flags);
    static string Target(string share) { return "NASBackupManager/SMB/" + share.ToLowerInvariant(); }
    public static int Store(string share, string user, string password, bool guest) {
        byte[] bytes = Encoding.Unicode.GetBytes(password);
        IntPtr blob = bytes.Length==0 ? IntPtr.Zero : Marshal.AllocCoTaskMem(bytes.Length);
        try {
            if(bytes.Length>0) Marshal.Copy(bytes, 0, blob, bytes.Length);
            var c = new Credential { Type=1, TargetName=Target(share), UserName=user,
                CredentialBlob=blob, CredentialBlobSize=(uint)bytes.Length, Persist=2,
                Comment=guest ? "NASBackupManager:guest:v1" : null };
            return Write(ref c,0) ? 0 : Marshal.GetLastWin32Error();
        } finally {
            for(int n=0; n<bytes.Length; n++) Marshal.WriteByte(blob,n,0);
            Array.Clear(bytes,0,bytes.Length); Marshal.FreeCoTaskMem(blob);
        }
    }
    public static int Exists(string share) {
        IntPtr ptr;
        if (!Read(Target(share),1,0,out ptr)) return Marshal.GetLastWin32Error();
        CredFree(ptr); return 0;
    }
    public static int Profile(string share) {
        IntPtr ptr;
        if (!Read(Target(share),1,0,out ptr)) {
            int error=Marshal.GetLastWin32Error(); return error==1168 ? 100000 : error;
        }
        try {
            var c=(Credential)Marshal.PtrToStructure(ptr,typeof(Credential));
            if(c.Comment=="NASBackupManager:guest:v1") return c.UserName=="guest" && c.CredentialBlobSize==0 ? 100001 : 99999;
            return 100002;
        } finally { CredFree(ptr); }
    }
    public static int Forget(string share) {
        if (Delete(Target(share),1,0)) return 0;
        int error=Marshal.GetLastWin32Error(); return error==1168 ? 0 : error;
    }
    public static int Connect(string share, string user, string password, bool fromVault) {
        IntPtr ptr = IntPtr.Zero;
        bool guest=false;
        try {
            if (fromVault) {
                if (Read(Target(share),1,0,out ptr)) {
                    var c=(Credential)Marshal.PtrToStructure(ptr,typeof(Credential));
                    guest=c.Comment=="NASBackupManager:guest:v1";
                    user=guest ? "guest" : c.UserName;
                    // Empty is intentional: NULL would use the Windows default password.
                    password=guest || c.CredentialBlobSize==0 ? String.Empty : Marshal.PtrToStringUni(c.CredentialBlob,(int)c.CredentialBlobSize/2);
                } else {
                    int readError=Marshal.GetLastWin32Error();
                    if(readError!=1168) return readError;
                    user=null; password=null; // Try Windows/domain credentials without a prompt.
                }
            }
            var resource=new Resource { Type=1, RemoteName=share };
            int error=(int)Add(ref resource,password,user,0);
            if(guest && (error==5 || error==86 || error==1326 || error==1909)) return 100003;
            return error;
        } finally { if(ptr!=IntPtr.Zero) CredFree(ptr); }
    }
}
'@
    $share = [string]$request.share
    $code = 87
    switch ([string]$request.action) {
        'profile' { $code = [NasBackupNetwork]::Profile($share) }
        'store_guest' { $code = [NasBackupNetwork]::Store($share,'guest','',$true) }
        'status' { $code = [NasBackupNetwork]::Exists($share) }
        'store' { $code = [NasBackupNetwork]::Store($share,[string]$request.username,[string]$request.password,$false) }
        'forget' { $code = [NasBackupNetwork]::Forget($share) }
        'connect' { $code = [NasBackupNetwork]::Connect($share,$null,$null,$true) }
        'save' {
            $code = [NasBackupNetwork]::Connect($share,[string]$request.username,[string]$request.password,$false)
            if ($code -eq 0) { $code = [NasBackupNetwork]::Store($share,[string]$request.username,[string]$request.password,$false) }
        }
    }
    [Console]::WriteLine([string]$code)
} catch {
    # Neither exception text nor request contents can leave this helper.
    [Console]::WriteLine('99999')
}
