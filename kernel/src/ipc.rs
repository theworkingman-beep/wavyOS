//! Simple IPC mailbox (stub v0)
use alloc::collections::vec_deque::VecDeque;
use spin::Mutex;
use crate::scheduler::TaskId;

pub const IPC_PAYLOAD_SIZE: usize = 64;

#[derive(Debug, Clone, Copy)]
pub struct IpcMessage {
    pub sender: TaskId,
    pub msg_type: u8,
    pub payload: [u8; IPC_PAYLOAD_SIZE],
}

static MAILBOXES: Mutex<VecDeque<(TaskId, IpcMessage)>> = Mutex::new(VecDeque::new());

pub fn init() {}

pub fn send(target: TaskId, msg: IpcMessage) {
    MAILBOXES.lock().push_back((target, msg));
}

pub fn recv(_who: TaskId) -> Option<IpcMessage> {
    let mut q = MAILBOXES.lock();
    for i in 0..q.len() {
        if q[i].0 == _who {
            return Some(q.remove(i).unwrap().1);
        }
    }
    None
}
