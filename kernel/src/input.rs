use spin::Mutex;

pub const INPUT_BUF_SIZE: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEvent {
    MouseMove { x: u16, y: u16, buttons: u8 },
    MouseDown { button: u8, x: u16, y: u16 },
    MouseUp { button: u8, x: u16, y: u16 },
    KeyPress { ascii: u8 },
}

struct InputQueue {
    buffer: [Option<InputEvent>; INPUT_BUF_SIZE],
    head: usize,
    tail: usize,
}

impl InputQueue {
    const fn new() -> Self {
        Self {
            buffer: [const { None }; INPUT_BUF_SIZE],
            head: 0,
            tail: 0,
        }
    }

    fn push(&mut self, event: InputEvent) {
        let next = (self.head + 1) % INPUT_BUF_SIZE;
        if next != self.tail {
            self.buffer[self.head] = Some(event);
            self.head = next;
        }
    }

    fn poll(&mut self) -> Option<InputEvent> {
        if self.head == self.tail {
            None
        } else {
            let event = self.buffer[self.tail].take();
            self.tail = (self.tail + 1) % INPUT_BUF_SIZE;
            event
        }
    }
}

static INPUT_QUEUE: Mutex<InputQueue> = Mutex::new(InputQueue::new());

/// Initialize the input subsystem (called once during kernel boot)
pub fn init() {
    log::info!("input: initialized");
}

/// Push an input event from an IRQ handler or driver
pub fn push(event: InputEvent) {
    INPUT_QUEUE.lock().push(event);
}

/// Poll the next pending input event (non-blocking, called by gui_task)
pub fn poll() -> Option<InputEvent> {
    INPUT_QUEUE.lock().poll()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_poll() {
        let mut queue = InputQueue::new();
        queue.push(InputEvent::KeyPress { ascii: b'a' });
        assert_eq!(queue.poll(), Some(InputEvent::KeyPress { ascii: b'a' }));
        assert_eq!(queue.poll(), None);
    }

    #[test]
    fn test_ring_buffer_overflow() {
        let mut queue = InputQueue::new();
        for i in 0..300usize {
            queue.push(InputEvent::KeyPress { ascii: (i % 256) as u8 });
        }
        let mut count = 0;
        while queue.poll().is_some() {
            count += 1;
        }
        assert!(count <= INPUT_BUF_SIZE);
    }

    #[test]
    fn test_mouse_events() {
        let mut queue = InputQueue::new();
        queue.push(InputEvent::MouseMove { x: 100, y: 200, buttons: 0 });
        queue.push(InputEvent::MouseDown { button: 0, x: 100, y: 200 });
        queue.push(InputEvent::MouseUp { button: 0, x: 100, y: 200 });

        assert_eq!(queue.poll(), Some(InputEvent::MouseMove { x: 100, y: 200, buttons: 0 }));
        assert_eq!(queue.poll(), Some(InputEvent::MouseDown { button: 0, x: 100, y: 200 }));
        assert_eq!(queue.poll(), Some(InputEvent::MouseUp { button: 0, x: 100, y: 200 }));
        assert_eq!(queue.poll(), None);
    }
}
