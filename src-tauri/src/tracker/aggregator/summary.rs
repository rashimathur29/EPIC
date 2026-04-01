use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryData {
    pub start_time: String,         
    pub end_time: String,            
    pub keystroke_list: Vec<u32>,    
    pub mouse_movement_list: Vec<u32>, 
    pub mouse_click_list: Vec<u32>,  
    pub total_idle_seconds: u32,     
    pub minute_ids: Vec<i64>,
}

pub struct SummaryGenerator;

impl SummaryGenerator {
    pub fn generate(minutes: &[super::minute::MinuteEntry]) -> Option<SummaryData> {
        if minutes.is_empty() {
            return None;
        }
        
        let keystroke_list: Vec<u32> = minutes.iter()
            .map(|m| m.data.keystroke_count)
            .collect();
        
        let mouse_movement_list: Vec<u32> = minutes.iter()
            .map(|m| m.data.mouse_move_count)
            .collect();
        
        let mouse_click_list: Vec<u32> = minutes.iter()
            .map(|m| m.data.mouse_click_count)
            .collect();
        
        let minute_ids: Vec<i64> = minutes.iter()
            .map(|m| m.id)
            .collect();
        
        let total_idle_seconds: u32 = minutes.iter()
            .map(|m| m.data.idle_seconds)
            .sum();
        
        Some(SummaryData {
            start_time: minutes.first().unwrap().data.minute_start.clone(),
            end_time: minutes.last().unwrap().data.minute_end.clone(),
            keystroke_list,
            mouse_movement_list,
            mouse_click_list,
            total_idle_seconds,
            minute_ids,
        })
    }
}