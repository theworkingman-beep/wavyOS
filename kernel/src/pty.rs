//! PTY (Pseudo-Terminal) driver for VibeOS
//! Manages PTY master/slave pairs with buffered data transfer.

use alloc::collections::vec_deque::VecDeque;
use alloc::vec::Vec;
use spin::Mutex;

/// Size of the PTY buffer in each direction
pub const PTY_BUF_SIZE: usize = 4096;

/// A single PTY session (master/slave pair)
pub struct PtySession {
    pub id: usize,
    /// Buffer for data written by master, to be read by slave (master->slave direction)
    pub master_to_slave: VecDeque<u8>,
    /// Buffer for data written by slave, to be read by master (slave->master direction)
    pub slave_to_master: VecDeque<u8>,
    /// PID of the process connected to the slave side (0 if unassigned)
    pub slave_pid: usize,
    /// PID of the process that owns the master side (creator)
    pub master_pid: usize,
}

impl PtySession {
    pub fn new(id: usize, master_pid: usize) -> Self {
        PtySession {
            id,
            master_to_slave: VecDeque::with_capacity(PTY_BUF_SIZE),
            slave_to_master: VecDeque::with_capacity(PTY_BUF_SIZE),
            slave_pid: 0,
            master_pid,
        }
    }

    /// Write data from the master side (will be read by slave).
    /// Returns the number of bytes actually written.
    pub fn master_write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        for &byte in data {
            if self.master_to_slave.len() >= PTY_BUF_SIZE {
                break;
            }
            self.master_to_slave.push_back(byte);
            written += 1;
        }
        written
    }

    /// Read data from the master side (data produced by slave).
    /// Returns the number of bytes actually read.
    pub fn master_read(&mut self, buf: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in buf.iter_mut() {
            if let Some(b) = self.slave_to_master.pop_front() {
                *byte = b;
                read += 1;
            } else {
                break;
            }
        }
        read
    }

    /// Write data from the slave side (will be read by master).
    /// Returns the number of bytes actually written.
    pub fn slave_write(&mut self, data: &[u8]) -> usize {
        let mut written = 0;
        for &byte in data {
            if self.slave_to_master.len() >= PTY_BUF_SIZE {
                break;
            }
            self.slave_to_master.push_back(byte);
            written += 1;
        }
        written
    }

    /// Read data from the slave side (data produced by master / keyboard input).
    /// Returns the number of bytes actually read.
    pub fn slave_read(&mut self, buf: &mut [u8]) -> usize {
        let mut read = 0;
        for byte in buf.iter_mut() {
            if let Some(b) = self.master_to_slave.pop_front() {
                *byte = b;
                read += 1;
            } else {
                break;
            }
        }
        read
    }
}

/// Global PTY table
static PTY_TABLE: Mutex<Vec<PtySession>> = Mutex::new(Vec::new());
static NEXT_PTY_ID: Mutex<usize> = Mutex::new(1);

pub fn init() {
    log::info!("pty: initialized");
}

/// Create a new PTY master/slave pair. Returns the PTY ID.
/// The calling process (master_pid) owns the master side.
pub fn pty_open(master_pid: usize) -> usize {
    let id = {
        let mut next = NEXT_PTY_ID.lock();
        let v = *next;
        *next += 1;
        v
    };
    let session = PtySession::new(id, master_pid);
    PTY_TABLE.lock().push(session);
    log::info!("pty: opened PTY id={} master_pid={}", id, master_pid);
    id
}

/// Read from PTY master (reads data produced by slave).
/// Returns number of bytes read, or 0 if PTY not found or no data.
pub fn pty_master_read(pty_id: usize, buf: &mut [u8]) -> usize {
    let mut table = PTY_TABLE.lock();
    if let Some(session) = table.iter_mut().find(|s| s.id == pty_id) {
        session.master_read(buf)
    } else {
        0
    }
}

/// Write to PTY master (data goes to slave, e.g. keyboard input).
/// Returns number of bytes written.
pub fn pty_master_write(pty_id: usize, data: &[u8]) -> usize {
    let mut table = PTY_TABLE.lock();
    if let Some(session) = table.iter_mut().find(|s| s.id == pty_id) {
        session.master_write(data)
    } else {
        0
    }
}

/// Read from PTY slave (reads data produced by master, e.g. keyboard input).
/// Only accessible by the process assigned to the slave side.
/// Returns number of bytes read.
pub fn pty_slave_read(pty_id: usize, buf: &mut [u8]) -> usize {
    let mut table = PTY_TABLE.lock();
    if let Some(session) = table.iter_mut().find(|s| s.id == pty_id) {
        session.slave_read(buf)
    } else {
        0
    }
}

/// Write to PTY slave (shell output, goes to master for reading).
/// Returns number of bytes written.
pub fn pty_slave_write(pty_id: usize, data: &[u8]) -> usize {
    let mut table = PTY_TABLE.lock();
    if let Some(session) = table.iter_mut().find(|s| s.id == pty_id) {
        session.slave_write(data)
    } else {
        0
    }
}

/// Assign a process to the slave side of a PTY.
pub fn pty_assign_slave(pty_id: usize, slave_pid: usize) -> bool {
    let mut table = PTY_TABLE.lock();
    if let Some(session) = table.iter_mut().find(|s| s.id == pty_id) {
        session.slave_pid = slave_pid;
        log::info!("pty: assigned slave pid={} to PTY id={}", slave_pid, pty_id);
        true
    } else {
        false
    }
}

/// Find the PTY that a given PID is the slave of.
/// Returns PTY ID if found, 0 otherwise.
pub fn pty_find_by_slave_pid(slave_pid: usize) -> usize {
    let table = PTY_TABLE.lock();
    for session in table.iter() {
        if session.slave_pid == slave_pid {
            return session.id;
        }
    }
    0
}

/// Find the PTY that a given PID is the master of.
/// Returns PTY ID if found, 0 otherwise.
pub fn pty_find_by_master_pid(master_pid: usize) -> usize {
    let table = PTY_TABLE.lock();
    for session in table.iter() {
        if session.master_pid == master_pid {
            return session.id;
        }
    }
    0
}