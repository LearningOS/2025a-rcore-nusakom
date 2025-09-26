//! Process management syscalls

use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next};
use crate::mm::frame_alloc;
use crate::mm::memory_set::{MapPermission, MemorySet}; // 用 MemorySet 替代 map_one/unmap_one
use crate::timer::get_time_ms;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");

    let ts_ref = unsafe { (ts as *mut TimeVal).as_mut() };
    if ts_ref.is_none() { return -1; }
    let ts_ref = ts_ref.unwrap();

    let time = get_time_ms();
    ts_ref.sec = time / 1000;
    ts_ref.usec = (time % 1000) * 1000;
    0
}

/// read/write user memory for tracing
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");

    match trace_request {
        0 => {
            let r = unsafe { MemorySet::translate(id) };
            match r {
                Some(pte) => pte.get_byte() as isize,
                None => -1,
            }
        },
        1 => {
            let r = unsafe { MemorySet::translate_mut(id) };
            match r {
                Some(pte) => { pte.set_byte(data as u8); 0 },
                None => -1,
            }
        },
        _ => -1,
    }
}

/// map memory pages
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap");

    if start % 4096 != 0 || prot & !0x7 != 0 || prot & 0x7 == 0 {
        return -1;
    }

    let len = (len + 4095) / 4096 * 4096;

    let mut perm = MapPermission::U;
    if prot & 0x1 != 0 { perm |= MapPermission::R; }
    if prot & 0x2 != 0 { perm |= MapPermission::W; }
    if prot & 0x4 != 0 { perm |= MapPermission::X; }

    for addr in (start..start + len).step_by(4096) {
        let frame = match frame_alloc() {
            Some(f) => f,
            None => return -1,
        };
        MemorySet::map_one(addr, frame.ppn, perm);
    }

    0
}

/// unmap memory pages
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");

    if start % 4096 != 0 { return -1; }
    let len = (len + 4095) / 4096 * 4096;

    for addr in (start..start + len).step_by(4096) {
        MemorySet::unmap_one(addr);
    }

    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
