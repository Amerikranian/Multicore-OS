use core::ffi::CStr;

use crate::{
<<<<<<< HEAD
    constants::syscalls::{SYSCALL_EXIT, SYSCALL_PRINT},
    events::{current_running_event_info, spawn, yield_now, EventInfo, JoinHandle},
    interrupts::x2apic,
    processes::process::{clear_process_frames, ProcessState, PROCESS_TABLE},
=======
    constants::syscalls::*,
    events::{current_running_event_info, EventInfo},
    interrupts::{gdt::TSSS, x2apic::current_core_id},
    processes::process::{clear_process_frames, sleep_process, ProcessState, PROCESS_TABLE},
>>>>>>> src/main
    serial_println,
};
use alloc::{boxed::Box, collections::BTreeMap};
use core::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
use spin::RwLock;

<<<<<<< HEAD
#[derive(Debug)]
pub enum Error {
    InvalidCall,
    WouldBlock,
    NamespaceError,
}

// Store the syscall state for a process
struct SyscallState {
    join_handle: JoinHandle<Result<usize, Error>>,
    waker: Option<Waker>,
}

pub struct SyscallHandler {
    // Map of PIDs to their syscall state
    syscalls: RwLock<BTreeMap<u32, SyscallState>>,
}

impl SyscallHandler {
    pub const fn new() -> Self {
        Self {
            syscalls: RwLock::new(BTreeMap::new()),
        }
    }

    // Start a syscall and block the process
    pub fn start_syscall(
        &self,
        pid: u32,
        handle: JoinHandle<Result<usize, Error>>,
    ) -> Poll<Result<usize, Error>> {
        self.syscalls.write().insert(
            pid,
            SyscallState {
                join_handle: handle,
                waker: None,
            },
        );
        Poll::Pending
    }
}

static SYSCALL_HANDLER: SyscallHandler = SyscallHandler::new();

// Future that waits for syscall completion
struct SyscallFuture {
    pid: u32,
}

impl Future for SyscallFuture {
    type Output = Result<usize, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut syscalls = SYSCALL_HANDLER.syscalls.write();

        if let Some(state) = syscalls.get_mut(&self.pid) {
            // Poll the join handle
            let result = Pin::new(&mut state.join_handle).poll(cx);

            match result {
                Poll::Ready(Ok(syscall_result)) => {
                    // Syscall completed, remove state and return result
                    syscalls.remove(&self.pid);
                    Poll::Ready(syscall_result)
                }
                Poll::Ready(Err(_)) => {
                    // Handle task error
                    syscalls.remove(&self.pid);
                    Poll::Ready(Err(Error::InvalidCall))
                }
                Poll::Pending => {
                    // Store waker and keep waiting
                    state.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        } else {
            Poll::Ready(Err(Error::InvalidCall))
        }
    }
}

#[no_mangle]
pub extern "C" fn dispatch_syscall() {
    let (syscall_num, arg1, arg2, arg3): (u32, usize, usize, usize);

    unsafe {
        core::arch::asm!(
            "mov {0:r}, rax",
            "mov {1}, rdi",
            "mov {2}, rsi",
            "mov {3}, rdx",
            out(reg) syscall_num,
            out(reg) arg1,
            out(reg) arg2,
            out(reg) arg3,
        );
    }

    let cpuid = x2apic::current_core_id() as u32;
    let event = current_running_event_info(cpuid);
    let pid = event.pid;

    let handler = match syscall_num {
        SYSCALL_PRINT => {
            let fut: Pin<Box<dyn Future<Output = Result<usize, Error>> + Send>> =
                Box::pin(handle_read(pid, arg1, arg2, arg3));
            Some(fut)
        }
        SYSCALL_EXIT => {
            sys_exit();
            None
        }
        _ => None,
    };

    if let Some(handler) = handler {
        // Spawn the handler and get join handle
        let join_handle = spawn(cpuid, handler, 1);

        // Start syscall and block process
        let _ = SYSCALL_HANDLER.start_syscall(pid, join_handle);

        // Create future to wait for completion
        let syscall_future = SyscallFuture { pid };

        // Spawn the waiting future
        spawn(cpuid, syscall_future, 1);
=======
#[warn(unused)]
use crate::interrupts::x2apic;
#[allow(unused)]
use core::arch::naked_asm;

#[repr(C)]
#[derive(Debug)]
pub struct SyscallRegisters {
    pub number: u64, // syscall number (originally in rax)
    pub arg1: u64,   // originally in rdi
    pub arg2: u64,   // originally in rsi
    pub arg3: u64,   // originally in rdx
    pub arg4: u64,   // originally in r10
    pub arg5: u64,   // originally in r8
    pub arg6: u64,   // originally in r9
}

#[no_mangle]
fn get_ring_0_rsp() -> u64 {
    let core = current_core_id();
    ((TSSS[core].privilege_stack_table[0]).as_u64()) & !15
}

#[naked]
#[no_mangle]
pub extern "C" fn syscall_handler_64_naked() {
    unsafe {
        core::arch::naked_asm!(
            "
            swapgs
            mov r12, rcx
            mov r13, r11
            mov r14, rax // syscall num
            mov r15, rsp
            push rdi
            push rsi
            push rdx
            push r10
            push r8
            push r9
            call get_ring_0_rsp // call
            pop r9
            pop r8
            pop r10
            pop rdx
            pop rsi
            pop rdi
            mov rsp, rax // rax has return from get_ring_0_rsp, mov rsp
            mov rax, r14 // return rax (syscall_number) back
            sub rsp, 56
            mov [rsp + 0], rax
            mov [rsp + 8], rdi
            mov [rsp + 16], rsi
            mov [rsp + 24], rdx
            mov [rsp + 32], r10
            mov [rsp + 40], r8
            mov [rsp + 48], r9
            mov rdi, rsp
            call syscall_handler_impl
            add rsp, 56
            pop rbx
            mov rcx, r12
            mov r11, r13
            mov rsp, r15
            swapgs
            sysretq
            "
        )
    };
}

/// Function that routes to different syscalls
///
/// # Arguments
/// * `syscall` - A pointer to a strut containing syscall_num, arg1...arg6 as u64
///
/// # Safety
/// This function is unsafe as it must dereference `syscall` to get args
#[no_mangle]
pub unsafe extern "C" fn syscall_handler_impl(syscall: *const SyscallRegisters) -> u64 {
    let syscall = unsafe { &*syscall };
    serial_println!("Syscall num: {}", syscall.number);
    match syscall.number as u32 {
        SYSCALL_EXIT => {
            sys_exit(syscall.arg1 as i64);
            unreachable!("sys_exit does not return");
        }
        SYSCALL_PRINT => sys_print(syscall.arg1 as *const u8),
        SYSCALL_NANOSLEEP => sys_nanosleep(syscall.arg1, syscall.arg2),
        _ => {
            panic!("Unknown syscall, {}", syscall.number);
        }
>>>>>>> src/main
    }

    yield_now();
}

async fn handle_read(_: u32, _: usize, _: usize, _: usize) -> Result<usize, Error> {
    serial_println!("Hello, world!");
    Ok(0)
}

pub fn sys_exit(code: i64) {
    // TODO handle hierarchy (parent processes), resources, threads, etc.
    // TODO recursive page table walk to handle cleaning up process memory

    // Used for testing
    if code == -1 {
        panic!("Exitted with code -1");
    }

    let event: EventInfo = current_running_event_info();

    serial_println!("Process {} exitted with code {}", event.pid, code);

    if event.pid == 0 {
        panic!("Calling exit from outside of process");
    }

    // Get PCB from PID
    let preemption_info = unsafe {
        let mut process_table = PROCESS_TABLE.write();
        let process = process_table
            .get_mut(&event.pid)
            .expect("Process not found");

        let pcb = process.pcb.get();

        (*pcb).state = ProcessState::Terminated;
        clear_process_frames(&mut *pcb);
        process_table.remove(&event.pid);
        ((*pcb).kernel_rsp, (*pcb).kernel_rip)
    };

    unsafe {
        // Restore kernel RSP + PC -> RIP from where it was stored in run/resume process
        core::arch::asm!(
            "mov rsp, {0}",
            "push {1}",
            "ret",
            in(reg) preemption_info.0,
            in(reg) preemption_info.1
        );
    }
}

// Not a real system call, but useful for testing
pub fn sys_print(buffer: *const u8) -> u64 {
    let c_str = unsafe { CStr::from_ptr(buffer as *const i8) };
    let str_slice = c_str.to_str().expect("Invalid UTF-8 string");
    serial_println!("Buffer: {}", str_slice);

    0
}

pub fn sys_nanosleep(nanos: u64, rsp: u64) -> u64 {
    sleep_process(rsp, nanos);
    x2apic::send_eoi();

    0
}
