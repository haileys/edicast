use std::sync::{Arc, RwLock};

use crossbeam_channel::{Receiver, Sender, TrySendError};

const BUFFER_SIZE: usize = 1;

struct LiveChannel<T> {
    txs: RwLock<Option<Vec<Sender<T>>>>,
}

pub struct LivePublisher<T> {
    chan: Arc<LiveChannel<T>>,
}

pub struct LiveSubscriber<T> {
    chan: Arc<LiveChannel<T>>,
}

pub fn live_channel<T>() -> (LivePublisher<T>, LiveSubscriber<T>) {
    let chan = Arc::new(LiveChannel {
        txs: RwLock::new(Some(Vec::new())),
    });

    let publisher = LivePublisher { chan: Arc::clone(&chan) };
    let subscriber = LiveSubscriber { chan: Arc::clone(&chan) };

    (publisher, subscriber)
}

impl<T> LivePublisher<T> where T: Clone {
    pub fn publish(&self, data: T) {
        let mut dead_txs = Vec::new();

        let mut txs_lock = self.chan.txs.write()
            .expect("writer lock on txs");

        let txs = txs_lock.as_mut()
            .expect("txs should always be Some while LivePublisher alive");

        for (index, tx) in txs.iter().enumerate() {
            match tx.try_send(data.clone()) {
                Ok(()) => {}
                Err(TrySendError::Full(_)) => {
                    // receiver is not keeping up with the data, back off for
                    // now and drop this packet
                }
                Err(TrySendError::Disconnected(_)) => {
                    dead_txs.push(index);
                }
            }
        }

        for dead_tx_index in dead_txs {
            txs.swap_remove(dead_tx_index);
        }
    }
}

impl<T> Drop for LivePublisher<T> {
    fn drop(&mut self) {
        *self.chan.txs.write().expect("writer lock on txs") = None;
    }
}

pub enum SubscribeError {
    NoPublisher,
}

impl<T> LiveSubscriber<T> where T: Clone {
    pub fn subscribe(&self) -> Result<Receiver<T>, SubscribeError> {
        let (tx, rx) = crossbeam_channel::bounded(BUFFER_SIZE);

        self.chan.txs.write()
            .expect("writer lock on txs")
            .as_mut()
            .ok_or(SubscribeError::NoPublisher)?
            .push(tx);

        Ok(rx)
    }
}
