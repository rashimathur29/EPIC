use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinuteData {
    pub minute_start: String,
    pub minute_end: String,
    pub keystroke_count: u32,
    pub mouse_move_count: u32,
    pub mouse_click_count: u32,
    pub idle_seconds: u32,
}

#[derive(Debug, Clone)]
pub struct MinuteEntry {
    pub id: i64,
    pub data: MinuteData,
}

pub struct MinuteAggregator {
    current_minute_start: DateTime<Utc>,
    current_data: MinuteData,
    minute_entries: Vec<MinuteEntry>,
    summary_window: usize,
}

impl MinuteAggregator {
    pub fn new(summary_window: usize) -> Self {
        let now = Utc::now();
        let minute_start = now;
        //let minute_start = now.with_second(0).unwrap().with_nanosecond(0).unwrap();
        
        Self {
            current_minute_start: minute_start,
            current_data: MinuteData {
                minute_start: minute_start.format("%Y-%m-%d %H:%M:%S").to_string(),
                minute_end: (minute_start + Duration::seconds(60))
                    .format("%Y-%m-%d %H:%M:%S").to_string(),
                keystroke_count: 0,
                mouse_move_count: 0,
                mouse_click_count: 0,
                idle_seconds: 0,
            },
            minute_entries: Vec::new(),
            summary_window,
        }
    }
    
    pub fn add_event(&mut self, event: crate::tracker::events::TrackEvent) {
        match event {
            crate::tracker::events::TrackEvent::Key => self.current_data.keystroke_count += 1,
            crate::tracker::events::TrackEvent::MouseMove => self.current_data.mouse_move_count += 1,
            crate::tracker::events::TrackEvent::MouseClick => self.current_data.mouse_click_count += 1,
            crate::tracker::events::TrackEvent::IdleTick => self.current_data.idle_seconds += 1,
        }
    }
    
    pub fn store_minute_with_id(&mut self, id: i64, data: MinuteData) {
        self.minute_entries.push(MinuteEntry { id, data });
        
        if self.minute_entries.len() > self.summary_window * 2 {
            self.minute_entries.remove(0);
        }
    }
    
    pub fn get_recent_for_summary(&self) -> Vec<MinuteEntry> {
        if self.minute_entries.len() >= self.summary_window {
            // Get the LAST N entries in their ORIGINAL chronological order
            let start_index = self.minute_entries.len() - self.summary_window;
            self.minute_entries[start_index..].to_vec()
        } else {
            Vec::new()
        }
    }
    
    pub fn clear_processed_minutes(&mut self, ids: &[i64]) {
        self.minute_entries.retain(|entry| !ids.contains(&entry.id));
    }
    
    pub fn should_flush(&self) -> bool {
        Utc::now() >= self.current_minute_start + Duration::seconds(60)
    }
    
    pub fn flush(&mut self) -> Option<MinuteData> {
        if self.should_flush() {
            let completed = self.current_data.clone();
            
            let next_minute_start = self.current_minute_start + Duration::seconds(60);
            self.current_minute_start = next_minute_start;
            
            self.current_data = MinuteData {
                minute_start: next_minute_start.format("%Y-%m-%d %H:%M:%S").to_string(),
                minute_end: (next_minute_start + Duration::seconds(60))
                    .format("%Y-%m-%d %H:%M:%S").to_string(),
                keystroke_count: 0,
                mouse_move_count: 0,
                mouse_click_count: 0,
                idle_seconds: 0,
            };
            
            Some(completed)
        } else {
            None
        }
    }
    
    pub fn get_current_data(&self) -> &MinuteData {
        &self.current_data
    }
}