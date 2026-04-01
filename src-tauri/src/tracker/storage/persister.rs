use crate::db::core::DbManager;
use crate::tracker::aggregator::{MinuteData, SummaryData};
use crate::Result;

pub trait ActivityPersister: Send + Sync {
    fn insert_minute(&self, data: &MinuteData) -> Result<i64>;
    fn insert_summary(&self, data: &SummaryData) -> Result<()>;
    fn delete_minutes(&self, ids: &[i64]) -> Result<()>;
}

pub struct DbActivityPersister {
    db: std::sync::Arc<DbManager>,
}

impl DbActivityPersister {
    pub fn new(db: std::sync::Arc<DbManager>) -> Self {
        Self { db }
    }
}

impl ActivityPersister for DbActivityPersister {
    fn insert_minute(&self, data: &MinuteData) -> Result<i64> {
        // We need to get the inserted ID, but your method doesn't return it
        // We'll need to modify your DbManager or use a separate query
        // For now, let's assume we have a method that returns ID
        
        // Insert the minute
        self.db.insert_minute_activity(
            &data.minute_start,
            &data.minute_end,
            data.keystroke_count as i32,
            data.mouse_move_count as i32,
            data.mouse_click_count as i32,
            data.idle_seconds as i32,
        )?;
        
        // Get the last inserted ID
        let id: i64 = self.db.conn.lock().unwrap().query_row(
            "SELECT last_insert_rowid()",
            [],
            |row| row.get(0),
        )?;
        
        Ok(id)
    }
    
    fn insert_summary(&self, data: &SummaryData) -> Result<()> {
        // Convert Vec<u32> to Vec<i32>
        let keystroke_list: Vec<i32> = data.keystroke_list.iter()
            .map(|&x| x as i32)
            .collect();
        
        let mouse_movement_list: Vec<i32> = data.mouse_movement_list.iter()
            .map(|&x| x as i32)
            .collect();
        
        let mouse_click_list: Vec<i32> = data.mouse_click_list.iter()
            .map(|&x| x as i32)
            .collect();
        
        self.db.insert_summary_with_count_lists(
            &data.start_time,
            &data.end_time,
            &keystroke_list,
            &mouse_movement_list,
            &mouse_click_list,
            data.total_idle_seconds as i32,
        )?;
        Ok(())
    }
    
    fn delete_minutes(&self, ids: &[i64]) -> Result<()> {
        self.db.delete_minutes_by_ids(ids)?;
        Ok(())
    }
}