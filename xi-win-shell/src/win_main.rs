// Copyright 2018 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Windows main loop.

use std::cell::RefCell;
use std::mem;
use std::ptr::null_mut;
use std::rc::Rc;
use winapi::shared::winerror::*;
use winapi::um::winbase::*;
use winapi::um::winnt::*;
use winapi::um::winuser::*;

#[derive(Clone, Default)]
pub struct RunLoopHandle(Rc<RefCell<RunLoopState>>);

#[derive(Default)]
struct RunLoopState {
    listeners: Vec<Listener>,
    idle: Vec<Box<IdleCallback>>,
}

struct Listener {
    h: HANDLE,
    callback: Box<FnMut()>,
}

pub struct RunLoop {
    handle: RunLoopHandle,
}

pub trait IdleCallback {
    fn call(self: Box<Self>);
}

impl<F: FnOnce()> IdleCallback for F {
    fn call(self: Box<F>) {
        (*self)()
    }
}

impl RunLoop {
    pub fn new() -> RunLoop {
        RunLoop {
            handle: Default::default(),
        }
    }

    /// Get a handle to the run loop state so a client can add listeners,
    /// etc.
    pub fn get_handle(&self) -> RunLoopHandle {
        self.handle.clone()
    }

    pub fn run(&mut self) {

        unsafe {
            loop {
                let mut handles = Vec::new();
                for listener in &self.handle.0.borrow().listeners {
                    handles.push(listener.h);
                }
                let len = handles.len() as u32;
                let has_idle = !self.handle.0.borrow().idle.is_empty();
                let res = MsgWaitForMultipleObjectsEx(
                    len,
                    handles.as_ptr(),
                    if has_idle { 0 } else { INFINITE },
                    QS_ALLEVENTS,
                    0
                );

                // Handle windows messages
                loop {
                    let mut msg = mem::uninitialized();
                    // Note: we could use PM_REMOVE here and avoid the GetMessage below
                    let res = PeekMessageW(&mut msg, null_mut(), 0, 0, PM_NOREMOVE);
                    if res == 0 {
                        break;
                    }
                    let res = GetMessageW(&mut msg, null_mut(), 0, 0);
                    if res <= 0 {
                        return;
                    }
                    TranslateMessage(&mut msg);
                    DispatchMessageW(&mut msg);
                }

                if res >= WAIT_OBJECT_0 && res < WAIT_OBJECT_0 + len {
                    let ix = (res - WAIT_OBJECT_0) as usize;
                    (&mut self.handle.0.borrow_mut().listeners[ix].callback)();
                } else if res == WAIT_TIMEOUT {
                    let idles = mem::replace(&mut self.handle.0.borrow_mut().idle, Vec::new());
                    for callback in idles {
                        callback.call();
                    }
                }
            }
        }
    }
}

/// Request to quit the application, exiting the runloop.
pub fn request_quit() {
    unsafe {
        PostQuitMessage(0);
    }
}

impl RunLoopHandle {
    /// Add a listener for a Windows handle. Considered unsafe because the
    /// handle must be valid.
    pub unsafe fn add_handler<F>(&self, h: HANDLE, callback: F)
        where F: FnMut() + 'static
    {
        let listener = Listener {
            h,
            callback: Box::new(callback),
        };
        self.0.borrow_mut().listeners.push(listener);
    }

    /// Add an idle handler, which is called (once) when the message loop
    /// is empty.
    pub fn add_idle<F>(&self, callback: F) where F: FnOnce() + 'static {
        self.0.borrow_mut().idle.push(Box::new(callback));
    }
}