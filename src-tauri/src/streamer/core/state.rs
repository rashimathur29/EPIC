#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StreamProfile {
    High,
    Medium,
    Low,
}

impl StreamProfile {
    pub fn fps(&self) -> u64 {
        match self {
            StreamProfile::High => 10,
            StreamProfile::Medium => 6,
            StreamProfile::Low => 3,
        }
    }
}

#[derive(Debug)]
pub struct StreamingState {
    pub profile: StreamProfile,
}

impl StreamingState {
    pub fn new(profile: StreamProfile) -> Self {
        Self { profile }
    }

    pub fn set_profile(&mut self, profile: StreamProfile) {
        self.profile = profile;
    }
}
