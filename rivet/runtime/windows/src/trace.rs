//! The trace of what the runtime did in this process. Nothing is written until a trace is switched
//! on by a file in the debug folder or by `RIVET_TRACE`.

use std::ffi::c_void;
use std::sync::OnceLock;

static TRACE: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Turns the experiment trace on. The switch is checked by the caller; the file is in the logs folder.
pub fn set_trace(path: std::path::PathBuf) {
    let _ = TRACE.set(path);
}

/// Diagnostic: logs access violations with their offset in the browser module, so a crash on a
/// new build shows which recorded address was wrong. Only while the trace is on.
pub unsafe fn trace_crashes() {
    #[repr(C)]
    struct Record {
        code: u32,
        flags: u32,
        chained: *mut Record,
        address: *mut c_void,
        parameters: u32,
        information: [usize; 15],
    }
    #[repr(C)]
    struct Pointers {
        record: *mut Record,
        context: *mut c_void,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn RtlCaptureStackBackTrace(
            skip: u32,
            count: u32,
            frames: *mut *mut c_void,
            hash: *mut u32,
        ) -> u16;
        fn GetModuleHandleExW(flags: u32, address: *const c_void, module: *mut *mut c_void) -> i32;
        fn GetModuleFileNameW(module: *mut c_void, name: *mut u16, size: u32) -> u32;
        fn AddVectoredExceptionHandler(
            first: u32,
            handler: unsafe extern "system" fn(*mut Pointers) -> i32,
        ) -> *mut c_void;
    }
    unsafe extern "system" fn handler(info: *mut Pointers) -> i32 {
        let record = &*(*info).record;
        // Access violations, other failures (illegal instruction, fast fail, divide by zero) and breakpoints.
        if record.code == 0xC000_0005
            || (record.code & 0xFFFF_0000 == 0xC000_0000 && record.code != 0xC000_0194)
            || record.code == 0x8000_0003
        {
            let rip = record.address as usize;
            let base = crate::chromium::image::recorded().map_or(0, |image| image.base);
            trace(&format!(
                "exception {:#x} at {rip:#x} (module offset {:#x}) touching {:#x}",
                record.code,
                rip.wrapping_sub(base),
                record.information[1]
            ));
            // A jump to a bad address leaves no frames to walk, so the words on the stack that look
            // like return addresses into the browser module show who made the call.
            if base != 0 {
                let rsp = *((*info).context.cast::<u8>().add(0x98) as *const usize);
                let returns: Vec<String> = (0..96usize)
                    .filter_map(|index| {
                        let word = *((rsp + index * 8) as *const usize);
                        (word > base && word - base < 0x2000_0000)
                            .then(|| format!("{:#x}", word - base))
                    })
                    .collect();
                trace(&format!(
                    "  stack returns into the browser: {}",
                    returns.join(" ")
                ));
                let mut own = std::ptr::null_mut();
                GetModuleHandleExW(0x4 | 0x2, handler as *const c_void, &mut own);
                let own = own as usize;
                let ours: Vec<String> = (0..160usize)
                    .filter_map(|index| {
                        let word = *((rsp + index * 8) as *const usize);
                        (word > own && word - own < 0x400_0000)
                            .then(|| format!("{:#x}", word - own))
                    })
                    .collect();
                trace(&format!(
                    "  stack returns into the runtime: {}",
                    ours.join(" ")
                ));
            }
            let mut frames = [std::ptr::null_mut::<c_void>(); 24];
            let count = RtlCaptureStackBackTrace(0, 24, frames.as_mut_ptr(), std::ptr::null_mut());
            for frame in &frames[..count as usize] {
                let mut module = std::ptr::null_mut();
                // GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | UNCHANGED_REFCOUNT
                GetModuleHandleExW(0x4 | 0x2, *frame, &mut module);
                let mut name = [0u16; 260];
                let length = GetModuleFileNameW(module, name.as_mut_ptr(), 260) as usize;
                let name = String::from_utf16_lossy(&name[..length]);
                trace(&format!(
                    "  frame {:#x} {} +{:#x}",
                    *frame as usize,
                    name.rsplit('\\').next().unwrap_or(""),
                    (*frame as usize).wrapping_sub(module as usize)
                ));
            }
        }
        0
    }
    AddVectoredExceptionHandler(1, handler);
    // A panic inside a hook would abort the browser without a word, so it is written down first.
    std::panic::set_hook(Box::new(|info| trace(&format!("panic: {info}"))));
}

pub fn tracing() -> bool {
    TRACE.get().is_some()
}

pub fn trace(message: &str) {
    use std::io::Write;
    let Some(path) = TRACE.get() else { return };
    // A long session would otherwise grow the trace without end, so it rotates at its size limit.
    rivet_core::paths::cap_log(path);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "{message}");
    }
}
