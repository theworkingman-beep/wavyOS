//! Syscall dispatch table
use core::arch::asm;

pub fn init() {}

#[repr(usize)]
pub enum Syscall {
    Exit = 0,
    Write = 1,
    Read = 2,
    Spawn = 3,
    Yield = 4,
    IpcSend = 5,
    IpcRecv = 6,
    ShmCreate = 7,
    ShmMap = 8,
    FramebufferMap = 9,
    MachOExec = 0x700,
}

pub unsafe fn dispatch(n: usize, a1: usize, a2: usize, a3: usize) -> usize {
    match n {
        0 => {
            // exit
            0
        }
        1 => {
            // write stub
            a2
        }
        2 => {
            // read stub
            0
        }
        5 => {
            // ipc_send(target_tid, msg_ptr) — v0 stub
            crate::ipc::send(crate::scheduler::current_task_id(), crate::ipc::IpcMessage {
                sender: crate::scheduler::current_task_id(),
                msg_type: 0,
                payload: [0u8; crate::ipc::IPC_PAYLOAD_SIZE],
            });
            0
        }
        6 => {
            // ipc_recv
            match crate::ipc::recv(crate::scheduler::current_task_id()) {
                Some(_) => 1,
                None => 0,
            }
        }
        7 => {
            match crate::shm::create(a1) {
                Some(id) => id,
                None => 0,
            }
        }
        8 => {
            match crate::shm::lookup(a1) {
                Some((start, _)) => start,
                None => 0,
            }
        }
        0x700 => crate::compat::macho::exec(a1 as *const u8, a2 as usize),
        _ => 0,
    }
}
