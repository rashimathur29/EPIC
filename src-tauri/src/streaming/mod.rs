// This file tells Rust: "the streaming folder contains these modules"
// Without this file, Rust cannot find streamer.rs, recorder.rs, pipeline.rs

pub mod capture;
pub mod streamer;
pub mod recorder;
pub mod pipeline;