//! Finds the Windows App Runtime. Pattern from windows-rs crates/libs/reactor/src/native/bootstrap.rs.

#[cfg(feature = "self-contained")]
pub fn bootstrap() -> windows_core::Result<()> {
    // The embedded manifest resolves WinRT classes to the DLLs staged beside the exe.
    crate::diag::trace("bootstrap skipped: self-contained build");
    Ok(())
}

#[cfg(not(feature = "self-contained"))]
pub fn bootstrap() -> windows_core::Result<()> {
    use crate::bindings::*;
    use windows_core::*;

    const FAMILY: PCWSTR = w!("Microsoft.WindowsAppRuntime.2_8wekyb3d8bbwe");
    // 2.6.4.0 packed as PACKAGE_VERSION, 16 bits per part.
    const MIN_VERSION: u64 = 2 << 48 | 6 << 32 | 4 << 16;
    const LIFETIME_PROCESS: i32 = 0;

    unsafe {
        let mut length = 0;
        let packaged = GetCurrentPackageFullName(&mut length, PWSTR::null());
        if packaged == ERROR_INSUFFICIENT_BUFFER {
            return Ok(());
        }
        let mut id = PWSTR::null();
        let hr = TryCreatePackageDependency(
            std::ptr::null_mut(),
            FAMILY,
            PACKAGE_VERSION {
                Anonymous: PACKAGE_VERSION_0 {
                    Version: MIN_VERSION,
                },
            },
            PackageDependencyProcessorArchitectures_X64
                | PackageDependencyProcessorArchitectures_Neutral,
            LIFETIME_PROCESS,
            PCWSTR::null(),
            0,
            &mut id,
        );
        crate::diag::trace(&format!("TryCreatePackageDependency hr={:#010x}", hr.0));
        if hr == STATEREPOSITORY_E_DEPENDENCY_NOT_RESOLVED {
            return Err(Error::new(
                hr,
                "Windows App Runtime 2.6.4 or newer (x64) is not installed",
            ));
        }
        hr.ok()?;
        let mut context = std::ptr::null_mut();
        let mut full_name = PWSTR::null();
        let hr = AddPackageDependency(PCWSTR(id.0), 0, 0, &mut context, &mut full_name);
        crate::diag::trace(&format!("AddPackageDependency hr={:#010x}", hr.0));
        _ = HeapFree(GetProcessHeap(), 0, id.0.cast());
        _ = HeapFree(GetProcessHeap(), 0, full_name.0.cast());
        hr.ok()
    }
}
