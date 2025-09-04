use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};

type Job = Box<dyn FnOnce() + Send + 'static>;

enum Message {
    Job(Job),
    Terminate,
}

pub struct ThreadPool {
    sender: mpsc::Sender<Message>,
    workers: Vec<JoinHandle<()>>,
}

impl ThreadPool {
    pub fn new(size: usize) -> Self {
        assert!(size > 0);
        let (tx, rx) = mpsc::channel::<Message>();
        let rx = Arc::new(Mutex::new(rx));
        let mut workers = Vec::with_capacity(size);
        for _ in 0..size {
            let rx_c = Arc::clone(&rx);
            workers.push(thread::spawn(move || loop {
                let msg = {
                    let rx = rx_c.lock().unwrap();
                    rx.recv()
                };
                match msg {
                    Ok(Message::Job(job)) => job(),
                    Ok(Message::Terminate) | Err(_) => break,
                }
            }));
        }
        ThreadPool { sender: tx, workers }
    }

    pub fn enqueue<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let _ = self.sender.send(Message::Job(Box::new(f)));
    }

    pub fn join(self) {
        // Signal termination
        for _ in &self.workers {
            let _ = self.sender.send(Message::Terminate);
        }
        for h in self.workers {
            let _ = h.join();
        }
    }
}

