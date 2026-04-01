use crossbeam_channel::{bounded, Receiver, Sender, unbounded};
use std::thread;
use crate::tracker::aggregator::{MinuteData, SummaryData};
use super::persister::ActivityPersister;

pub struct StorageWriter {
    command_tx: Sender<StorageCommand>,
}

impl StorageWriter {
    pub fn new<P: ActivityPersister + 'static>(persister: P) -> Self {
        let (command_tx, command_rx) = unbounded();
        
        thread::spawn(move || {
            let mut worker = StorageWorker::new(persister);
            worker.run(command_rx);
        });
        
        Self { command_tx }
    }
    
    pub fn insert_minute(&self, data: MinuteData) -> crate::Result<i64> {
        let (tx, rx) = bounded(1);
        
        self.command_tx.send(StorageCommand::InsertMinute(data, tx))
            .map_err(|e| crate::Error::WorkerError(format!("Send failed: {}", e)))?;
        
        rx.recv()
            .map_err(|e| crate::Error::WorkerError(format!("Receive failed: {}", e)))?
    }
    
    pub fn insert_summary(&self, data: SummaryData) -> crate::Result<()> {
        let (tx, rx) = bounded(1);
        
        self.command_tx.send(StorageCommand::InsertSummary(data, tx))
            .map_err(|e| crate::Error::WorkerError(format!("Send failed: {}", e)))?;
        
        rx.recv()
            .map_err(|e| crate::Error::WorkerError(format!("Receive failed: {}", e)))?
    }
    
    pub fn delete_minutes(&self, ids: Vec<i64>) -> crate::Result<()> {
        let (tx, rx) = bounded(1);
        
        self.command_tx.send(StorageCommand::DeleteMinutes(ids, tx))
            .map_err(|e| crate::Error::WorkerError(format!("Send failed: {}", e)))?;
        
        rx.recv()
            .map_err(|e| crate::Error::WorkerError(format!("Receive failed: {}", e)))?
    }
}

impl Clone for StorageWriter {
    fn clone(&self) -> Self {
        Self {
            command_tx: self.command_tx.clone(),
        }
    }
}

enum StorageCommand {
    InsertMinute(MinuteData, Sender<crate::Result<i64>>),
    InsertSummary(SummaryData, Sender<crate::Result<()>>),
    DeleteMinutes(Vec<i64>, Sender<crate::Result<()>>),
}

struct StorageWorker<P: ActivityPersister> {
    persister: P,
}

impl<P: ActivityPersister> StorageWorker<P> {
    fn new(persister: P) -> Self {
        Self { persister }
    }
    
    fn run(&mut self, rx: Receiver<StorageCommand>) {
        loop {
            match rx.recv() {
                Ok(StorageCommand::InsertMinute(data, tx)) => {
                    let result = self.persister.insert_minute(&data);
                    let _ = tx.send(result);
                }
                Ok(StorageCommand::InsertSummary(data, tx)) => {
                    let result = self.persister.insert_summary(&data);
                    let _ = tx.send(result);
                }
                Ok(StorageCommand::DeleteMinutes(ids, tx)) => {
                    let result = self.persister.delete_minutes(&ids);
                    let _ = tx.send(result);
                }
                Err(_) => break,
            }
        }
    }
}