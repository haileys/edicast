use std::ops::{Deref, DerefMut, Drop};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, sync_channel, SyncSender, Receiver};
use std::time::Instant;

pub struct RendezvousSender<T> {
    ready: Arc<AtomicBool>,
    send: SyncSender<T>,
}

pub struct RendezvousReceiver<T> {
    ready: Arc<AtomicBool>,
    recv: Receiver<T>,
}

pub enum SendError {
    Disconnected,
    Busy,
}

pub enum RecvError {
    Disconnected,
}

pub enum RecvTimeoutError {
    Timeout,
    Disconnected,
}

pub fn rendezvous<T>() -> (RendezvousSender<T>, RendezvousReceiver<T>) {
    let ready = Arc::new(AtomicBool::new(true));
    let (send, recv) = sync_channel(0);

    (RendezvousSender { ready: Arc::clone(&ready), send },
        RendezvousReceiver { ready: Arc::clone(&ready), recv })
}

impl<T> RendezvousSender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError> {
        // attempt to acquire the readiness of the receiver
        // if the AtomicBool is already false, that indicates that the
        // receiver is busy servicing another request for an unknown
        // period of time
        if !self.ready.swap(false, Ordering::Relaxed) {
            return Err(SendError::Busy);
        }

        match self.send.send(value) {
            Ok(()) => Ok(()),
            Err(_) => {
                // restore the readiness of the disconnected channel so that
                // subsequent sends see Disconnected, not Busy.
                // there is a race here where other threads might see Busy
                // while we were attempting to send the value, but that's only
                // a minor problem for our purposes
                self.ready.store(true, Ordering::Relaxed);
                Err(SendError::Disconnected)
            }
        }
    }
}

impl<T> RendezvousReceiver<T> {
    pub fn recv<'a>(&'a self) -> Result<RendezvousHandle<'a, T>, RecvError> {
        match self.recv.recv() {
            Ok(value) => Ok(RendezvousHandle { value, recv: self }),
            Err(_) => Err(RecvError::Disconnected),
        }
    }

    pub fn recv_deadline<'a>(&'a self, deadline: Instant) -> Result<RendezvousHandle<'a, T>, RecvTimeoutError> {
        let now = Instant::now();

        if now <= deadline {
            return Err(RecvTimeoutError::Timeout);
        }

        match self.recv.recv_timeout(deadline - now) {
            Ok(value) => Ok(RendezvousHandle { value, recv: self }),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(RecvTimeoutError::Timeout),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RecvTimeoutError::Disconnected),
        }
    }
}

pub struct RendezvousHandle<'a, T> {
    value: T,
    recv: &'a RendezvousReceiver<T>,
}

impl<'a, T> Deref for RendezvousHandle<'a, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<'a, T> DerefMut for RendezvousHandle<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}

impl<'a, T> Drop for RendezvousHandle<'a, T> {
    fn drop(&mut self) {
        self.recv.ready.store(true, Ordering::Relaxed)
    }
}
