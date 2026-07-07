//! Process management: spawning, monitoring, log capture, and shutdown of
//! managed servers. Built incrementally across step 3 (3a: log buffer).

pub mod command;
pub mod log_buffer;
pub mod running;
