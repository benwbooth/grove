//! Burning and ISO images through libisofs + libburn, linked (csrc/burn.c;
//! no xorriso command).

use std::ffi::{c_char, c_double, c_int, c_void, CStr, CString};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

type Progress = extern "C" fn(*mut c_void, c_double, *const c_char);

extern "C" {
    fn grove_burn(
        paths: *const *const c_char,
        targets: *const *const c_char,
        n: c_int,
        volid: *const c_char,
        device: *const c_char,
        iso_path: *const c_char,
        simulate: c_int,
        eject: c_int,
        cancel: *const AtomicI32,
        cb: Progress,
        user: *mut c_void,
        err: *mut c_char,
        errlen: c_int,
    ) -> c_int;
}

/// Each item: (path on disk, where on the disc: "/name", or "/" for a
/// folder's contents at the top). `device` "" writes `iso`.
pub struct Job<'a> {
    pub items: Vec<(String, String)>,
    pub volid: &'a str,
    pub device: &'a str,
    pub iso: &'a str,
    pub simulate: bool,
    pub eject: bool,
}

extern "C" fn trampoline(user: *mut c_void, fraction: c_double, phase: *const c_char) {
    let f = unsafe { &mut *(user as *mut &mut dyn FnMut(f64, &str)) };
    let p = if phase.is_null() { String::new() } else { unsafe { CStr::from_ptr(phase) }.to_string_lossy().into_owned() };
    f(fraction, &p);
}

/// Run it, calling `progress(fraction, phase)` (fraction -1: a new phase).
pub fn burn(job: &Job, cancel: &AtomicBool, mut progress: impl FnMut(f64, &str)) -> Result<(), String> {
    let cs = |s: &str| CString::new(s).map_err(|_| "A name with a NUL byte".to_string());
    let paths: Vec<CString> = job.items.iter().map(|(p, _)| cs(p)).collect::<Result<_, _>>()?;
    let targets: Vec<CString> = job.items.iter().map(|(_, t)| cs(t)).collect::<Result<_, _>>()?;
    let pp: Vec<*const c_char> = paths.iter().map(|c| c.as_ptr()).collect();
    let tp: Vec<*const c_char> = targets.iter().map(|c| c.as_ptr()).collect();
    let (vol, dev, iso) = (cs(job.volid)?, cs(job.device)?, cs(job.iso)?);
    // The C side polls an int; mirror the cancel flag into it from a watcher.
    let flag = std::sync::Arc::new(AtomicI32::new(0));
    let done = std::sync::Arc::new(AtomicBool::new(false));
    let cancel_ptr = cancel as *const AtomicBool as usize;
    let (f2, d2) = (flag.clone(), done.clone());
    let watcher = std::thread::spawn(move || {
        let c = unsafe { &*(cancel_ptr as *const AtomicBool) };
        while !d2.load(Ordering::Relaxed) {
            if c.load(Ordering::Relaxed) {
                f2.store(1, Ordering::Relaxed);
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
    });
    let mut cb: &mut dyn FnMut(f64, &str) = &mut progress;
    let mut err = vec![0u8; 1024];
    let rc = unsafe {
        grove_burn(
            pp.as_ptr(),
            tp.as_ptr(),
            pp.len() as c_int,
            vol.as_ptr(),
            dev.as_ptr(),
            iso.as_ptr(),
            job.simulate as c_int,
            job.eject as c_int,
            &*flag as *const AtomicI32,
            trampoline,
            &mut cb as *mut &mut dyn FnMut(f64, &str) as *mut c_void,
            err.as_mut_ptr() as *mut c_char,
            err.len() as c_int,
        )
    };
    done.store(true, Ordering::Relaxed);
    let _ = watcher.join();
    if rc == 0 {
        Ok(())
    } else {
        Err(CStr::from_bytes_until_nul(&err).map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|_| "The burn didn't finish.".into()))
    }
}
