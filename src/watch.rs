//! Watching folders for changes (inotify), coalesced: the callback gets the
//! folders that changed at most every 250 ms.

use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub fn watch(dirs: Vec<PathBuf>, stop: Arc<AtomicBool>, on_change: impl Fn(Vec<PathBuf>) + Send + 'static) {
    std::thread::Builder::new()
        .name("grove-watch".into())
        .spawn(move || unsafe {
            let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
            if fd < 0 {
                return;
            }
            let mut wds: HashMap<i32, PathBuf> = HashMap::new();
            for d in &dirs {
                let Ok(c) = CString::new(d.as_os_str().as_bytes()) else { continue };
                let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_FROM | libc::IN_MOVED_TO | libc::IN_CLOSE_WRITE | libc::IN_ATTRIB | libc::IN_DELETE_SELF | libc::IN_MOVE_SELF;
                let wd = libc::inotify_add_watch(fd, c.as_ptr(), mask);
                if wd >= 0 {
                    wds.insert(wd, d.clone());
                }
            }
            let mut buf = vec![0u8; 64 * 1024];
            let mut pending: HashSet<PathBuf> = HashSet::new();
            let mut first: Option<Instant> = None;
            while !stop.load(Ordering::Relaxed) {
                let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
                libc::poll(&mut pfd, 1, 100);
                loop {
                    let n = libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len());
                    if n <= 0 {
                        break;
                    }
                    let mut off = 0usize;
                    while off + std::mem::size_of::<libc::inotify_event>() <= n as usize {
                        let ev = &*(buf.as_ptr().add(off) as *const libc::inotify_event);
                        if let Some(d) = wds.get(&ev.wd) {
                            pending.insert(d.clone());
                            first.get_or_insert_with(Instant::now);
                        }
                        off += std::mem::size_of::<libc::inotify_event>() + ev.len as usize;
                    }
                }
                if let Some(t) = first {
                    if t.elapsed() >= Duration::from_millis(250) && !pending.is_empty() {
                        first = None;
                        on_change(pending.drain().collect());
                    }
                }
            }
            libc::close(fd);
        })
        .ok();
}
