fn main() {
    // Match Tauri's runtime linkage: no separately installed VC++ runtime for
    // headless backups. UCRT remains a standard Windows 10 system component.
    static_vcruntime::metabuild();
}
