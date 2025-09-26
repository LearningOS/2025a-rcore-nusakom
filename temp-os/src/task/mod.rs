//! Task management implementation

mod context;
mod switch;

use crate::loader::{get_app_data, get_num_app};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::vec::Vec;
use lazy_static::*;
use switch::__switch;
// use core::mem::MaybeUninit as _; // 避免 unused import 错误

pub use context::TaskContext;

/// Task status
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskStatus {
    Ready,
    Running,
    Exited,
}

/// Task Control Block
pub struct TaskControlBlock {
    pub task_cx: TaskContext,
    pub task_status: TaskStatus,
    pub app_id: usize,
    pub program_brk: usize,
    pub trap_cx: TrapContext,
}

impl TaskControlBlock {
    pub fn new(_app_data: &'static [u8], app_id: usize) -> Self { // 前缀 _ 避免 unused warning
        TaskControlBlock {
            task_cx: unsafe { core::mem::zeroed() },
            task_status: TaskStatus::Ready,
            app_id,
            program_brk: 0,
            trap_cx: unsafe { core::mem::zeroed() },
        }
    }

    pub fn get_user_token(&self) -> usize {
        0
    }

    pub fn get_trap_cx(&mut self) -> &mut TrapContext {
        &mut self.trap_cx
    }

    pub fn change_program_brk(&mut self, size: i32) -> Option<usize> {
        let old_brk = self.program_brk;
        if size >= 0 {
            self.program_brk += size as usize;
        } else {
            self.program_brk = self.program_brk.saturating_sub((-size) as usize);
        }
        Some(old_brk)
    }
}

/// Task manager
pub struct TaskManager {
    num_app: usize,
    inner: UPSafeCell<TaskManagerInner>,
}

struct TaskManagerInner {
    tasks: Vec<TaskControlBlock>,
    current_task: usize,
}

lazy_static! {
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let mut tasks: Vec<TaskControlBlock> = Vec::new();
        for i in 0..num_app {
            tasks.push(TaskControlBlock::new(get_app_data(i), i));
        }
        TaskManager {
            num_app,
            inner: unsafe { UPSafeCell::new(TaskManagerInner { tasks, current_task: 0 }) },
        }
    };
}

impl TaskManager {
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let next_task = &mut inner.tasks[0];
        next_task.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &next_task.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused: TaskContext = unsafe { core::mem::zeroed() };
        unsafe { __switch(&mut _unused as *mut _, next_task_cx_ptr) };
        panic!("unreachable in run_first_task!");
    }

    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Ready;
    }

    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].task_status = TaskStatus::Exited;
    }

    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    fn get_current_token(&self) -> usize {
        let inner = self.inner.exclusive_access();
        inner.tasks[inner.current_task].get_user_token()
    }

    // ⚡ 改为闭包访问 TrapContext 避免借用冲突
    fn with_current_trap_cx<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut TrapContext) -> R,
    {
        let mut inner = self.inner.exclusive_access();
        f(inner.tasks[inner.current_task].get_trap_cx())
    }

    fn change_current_program_brk(&self, size: i32) -> Option<usize> {
        let mut inner = self.inner.exclusive_access();
        let cur = inner.current_task;
        inner.tasks[cur].change_program_brk(size)
    }

    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;
            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            unsafe { __switch(current_task_cx_ptr, next_task_cx_ptr) };
        } else {
            panic!("All applications completed!");
        }
    }
}

/// API for syscall usage
pub fn exit_current_and_run_next() {
    TASK_MANAGER.run_next_task();
}

pub fn suspend_current_and_run_next() {
    TASK_MANAGER.run_next_task();
}

pub fn change_program_brk(size: i32) -> Option<usize> {
    TASK_MANAGER.change_current_program_brk(size)
}

pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

pub fn current_user_token() -> usize {
    TASK_MANAGER.get_current_token()
}

// ⚡ 使用闭包访问 TrapContext
pub fn current_trap_cx<F, R>(f: F) -> R
where
    F: FnOnce(&mut TrapContext) -> R,
{
    TASK_MANAGER.with_current_trap_cx(f)
}
