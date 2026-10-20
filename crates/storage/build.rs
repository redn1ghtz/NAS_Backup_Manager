fn main() {
    use sha2::{Digest, Sha256};
    use std::{fs, path::PathBuf};
    println!("cargo:rerun-if-changed=native/ActivityHost.cs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"));
    let platform = match std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("x86_64") => "x64",
        _ => panic!("Unsupported Windows notification host architecture"),
    };
    let compiler = root.join("Microsoft.NET/Framework/v4.0.30319/csc.exe");
    let output = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("nas-backup-notify.exe");
    let source = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap())
        .join("native")
        .join("ActivityHost.cs");
    // Framework csc has no deterministic mode. Share one original compilation
    // per source/compiler/profile so GUI and CLI expect the same sidecar digest.
    let mut key = Sha256::new();
    key.update(fs::read(&source).expect("notification source"));
    key.update(fs::read(&compiler).expect("installed Framework compiler"));
    key.update(format!("winexe,{platform},optimize:v2"));
    let key = format!("{:x}", key.finalize());
    let profile = output.ancestors().nth(4).expect("Cargo profile directory");
    let cache = profile.join("native-host").join(key);
    fs::create_dir_all(&cache).expect("native build cache");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(cache.join("build.lock"))
        .expect("native build lock");
    lock.lock().expect("exclusive native build lock");
    let cached_host = cache.join("nas-backup-notify.exe");
    if !cached_host.exists() {
        let result = std::process::Command::new(compiler)
            .args([
                "/nologo",
                "/target:winexe",
                "/optimize+",
                "/reference:System.Windows.Forms.dll",
                "/reference:System.Drawing.dll",
                "/reference:System.Web.Extensions.dll",
            ])
            .arg(format!("/platform:{platform}"))
            .arg(if platform == "x86" {
                "/define:HOST_X86"
            } else {
                "/define:HOST_X64"
            })
            .arg(format!("/out:{}", cached_host.display()))
            .arg(source)
            .output()
            .expect("Windows .NET Framework C# compiler");
        assert!(
            result.status.success(),
            "Activity host build failed: {} {}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fs::copy(&cached_host, &output).expect("copy shared notification build artifact");
    let installed_host = profile.join("nas-backup-notify.exe");
    let host_bytes = fs::read(&cached_host).expect("compiled notification sidecar");
    if fs::read(&installed_host).ok().as_deref() != Some(&host_bytes) {
        fs::copy(&cached_host, &installed_host)
            .expect("stage notification sidecar beside built executables");
    }
    let digest = Sha256::digest(fs::read(&output).expect("built notification host"));
    fs::write(
        output.with_file_name("activity_digest.rs"),
        format!("const HOST_SHA256: [u8; 32] = {:?};", &digest[..]),
    )
    .expect("notification host digest");
    // Build output only. Runtime must never unpack executable payloads to data/.
}
