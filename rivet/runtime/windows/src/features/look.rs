//! Rivet's own look inside the browser window: its colours, and a helper that lends a colour
//! provider Rivet's colours for one call.

pub(crate) const PAPER: u32 = 0xFFFB_FAF7;
pub(crate) const HAIRLINE: u32 = 0xFFE9_E9E6;
pub(crate) const INK: u32 = 0xFF1F_2024;
pub(crate) const SOFT_INK: u32 = 0xFF4A_4850;
/// The accent #9a5424 at 9 percent and at 25 percent over paper.
pub(crate) const ACCENT_TINT: u32 = 0xFFF2_EBE4;
pub(crate) const SELECTION: u32 = 0xFFE3_D0C2;
/// Keyboard focus keeps a visible ring on the toolbar's buttons, in ink instead of the browser's
/// blue. The address field has none: its fill is its focus.
pub(crate) const FOCUS_RING: u32 = 0x661F_2024;

/// Runs `work` with every colour of the provider at `provider` replaced by what `map` returns for
/// its id and value, then puts the provider's own values back. Used where a Rivet surface reads
/// colours outside a view's paint.
pub(crate) unsafe fn with_colour_map<R>(
    provider: *const u8,
    map: impl Fn(i32, u32) -> Option<u32>,
    work: impl FnOnce() -> R,
) -> R {
    fn collect(node: *mut u8, out: &mut Vec<*mut u8>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            collect(*(node as *const *mut u8), out, depth + 1);
            out.push(node);
            collect(*(node.add(8) as *const *mut u8), out, depth + 1);
        }
    }
    let mut nodes = Vec::new();
    collect(*(provider.add(0x18) as *const *mut u8), &mut nodes, 0);
    let mut saved = Vec::new();
    for node in nodes {
        let slot = node.add(0x24) as *mut u32;
        let original = slot.read();
        if let Some(colour) = map(*(node.add(0x20) as *const i32), original) {
            slot.write(colour);
            saved.push((slot, original));
        }
    }
    let result = work();
    for (slot, original) in saved {
        slot.write(original);
    }
    result
}

/// Traces every colour a provider holds as `id=value`, in id order, for a checked build.
pub(crate) unsafe fn dump_colours(provider: *const u8) {
    fn walk(node: *const u8, out: &mut Vec<String>, depth: usize) {
        if node.is_null() || depth > 40 {
            return;
        }
        unsafe {
            walk(*(node as *const *const u8), out, depth + 1);
            out.push(format!("{}={:08x}", *(node.add(0x20) as *const i32), *(node.add(0x24) as *const u32)));
            walk(*(node.add(8) as *const *const u8), out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(*(provider.add(0x18) as *const *const u8), &mut out, 0);
    crate::trace::trace(&format!("colours {} {}", out.len(), out.join(" ")));
}
