//! The trace of what the runtime did in this process. Nothing is written until a trace is switched
//! on by a file in the debug folder or by `RIVET_TRACE`.

use std::ffi::c_void;
use std::sync::OnceLock;

static TRACE: OnceLock<std::path::PathBuf> = OnceLock::new();

/// Turns the experiment trace on. The switch is checked by the caller; the file is in the logs folder.
pub fn set_trace(path: std::path::PathBuf) {
    let _ = TRACE.set(path);
}

/// Where the trace `name` writes in this process, `None` when it is not switched on. A switch is a
/// file in the session profile's own `rivet-debug` folder, a file in Rivet's `config\debug` folder
/// or the name in `RIVET_TRACE`. The profile is the one given by `--user-data-dir`, never a guess.
pub fn switched(name: &str) -> Option<std::path::PathBuf> {
    let layout = rivet_core::paths::Layout::current()?;
    let path = switched_in(
        &layout,
        name,
        std::env::var("RIVET_TRACE").ok().as_deref(),
        std::env::args(),
    )?;
    std::fs::create_dir_all(path.parent()?).ok()?;
    rivet_core::paths::cap_log(&path);
    Some(path)
}

fn switched_in(
    layout: &rivet_core::paths::Layout,
    name: &str,
    rivet_trace: Option<&str>,
    arguments: impl IntoIterator<Item = String>,
) -> Option<std::path::PathBuf> {
    let own = rivet_core::paths::session_debug_dir(arguments);
    layout.trace_file_with(name, rivet_trace, own.as_deref())
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
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    thread_local! {
        static INSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    unsafe extern "system" fn handler(info: *mut Pointers) -> i32 {
        let record = &*(*info).record;
        // A fault while this handler writes its trace would enter it again, and again, until the
        // stack runs out and the browser dies of the trace instead of its own exception.
        let nested = INSIDE
            .try_with(|inside| inside.replace(true))
            .unwrap_or(true);
        if !nested {
            if traced(record.code, false) {
                write_exception(info);
            }
            let _ = INSIDE.try_with(|inside| inside.set(false));
        }
        0
    }
    unsafe fn write_exception(info: *mut Pointers) {
        let record = &*(*info).record;
        {
            let (mut low, mut high) = (0usize, 0usize);
            GetCurrentThreadStackLimits(&mut low, &mut high);
            // Only words that lie on this thread's stack are read.
            let on_stack = |rsp: usize, count: usize| count.min(high.saturating_sub(rsp) / 8);
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
                let rsp = if (low..high).contains(&rsp) {
                    rsp
                } else {
                    high
                };
                let returns: Vec<String> = (0..on_stack(rsp, 96))
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
                let ours: Vec<String> = (0..on_stack(rsp, 160))
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
    }
    AddVectoredExceptionHandler(1, handler);
    // A panic inside a hook would abort the browser without a word, so it is written down first.
    std::panic::set_hook(Box::new(|info| trace(&format!("panic: {info}"))));
}

/// Whether the crash trace writes an exception down: access violations, other failures (illegal
/// instruction, fast fail, divide by zero) and breakpoints. A stack overflow leaves no room to write,
/// and an exception raised while the trace is being written is never traced again.
pub(crate) fn traced(code: u32, nested: bool) -> bool {
    !nested
        && code != 0xC000_00FD
        && (code == 0xC000_0005
            || (code & 0xFFFF_0000 == 0xC000_0000 && code != 0xC000_0194)
            || code == 0x8000_0003)
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

/// Writes `message` only when it differs from the last one written under `key`, so a trace inside
/// a layout that runs many times a second shows each change once.
pub fn trace_changed(key: &'static str, message: &str) {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static LAST: Mutex<Option<HashMap<&'static str, String>>> = Mutex::new(None);
    if !tracing() {
        return;
    }
    let mut last = LAST.lock().unwrap_or_else(|error| error.into_inner());
    let map = last.get_or_insert_with(HashMap::new);
    if map.get(key).is_some_and(|previous| previous == message) {
        return;
    }
    map.insert(key, message.to_owned());
    drop(last);
    trace(message);
}

#[cfg(test)]
mod tests {
    use super::{switched_in, traced};
    use rivet_core::paths::{Layout, TestDir, PROFILE_DEBUG_NAME};

    #[test]
    fn nfr7_a_trace_is_switched_on_only_by_a_debug_file_or_rivet_trace() {
        let local = TestDir::new("nfr7-runtime-trace");
        let layout = Layout::under(&local);
        let profile = local.join("profile");
        let args = || vec![format!("--user-data-dir={}", profile.display())];
        let name = "rivet-nav.log";
        assert_eq!(switched_in(&layout, name, None, args()), None);
        assert_eq!(
            switched_in(&layout, name, Some(name), args()),
            Some(layout.logs().join(name))
        );
        std::fs::create_dir_all(layout.debug()).unwrap();
        std::fs::write(layout.debug().join(name), "").unwrap();
        assert_eq!(
            switched_in(&layout, name, None, Vec::new()),
            Some(layout.logs().join(name))
        );
        let own = profile.join(PROFILE_DEBUG_NAME);
        std::fs::create_dir_all(&own).unwrap();
        std::fs::write(own.join(name), "").unwrap();
        assert_eq!(
            switched_in(&layout, name, None, args()),
            Some(own.join(name)),
            "a test profile's own switch keeps its trace in that profile"
        );
    }

    #[test]
    fn nfr7_the_crash_trace_never_traces_its_own_faults_or_a_stack_overflow() {
        assert!(traced(0xC000_0005, false));
        assert!(traced(0x8000_0003, false));
        assert!(traced(0xC000_0409, false));
        // A fault inside the trace would re-enter it until the stack is gone.
        assert!(!traced(0xC000_0005, true));
        assert!(!traced(0xC000_00FD, false));
        assert!(!traced(0xC000_0194, false));
        // Ordinary first chance exceptions a browser raises and handles are left alone.
        assert!(!traced(0x406D_1388, false));
        assert!(!traced(0xE06D_7363, false));
    }
}
