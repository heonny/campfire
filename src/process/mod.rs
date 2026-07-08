//! Process management: spawning, monitoring, log capture, and shutdown of
//! managed servers. Built incrementally across step 3 (3a: log buffer).

pub mod command;
pub mod kill_tree;
pub mod log_buffer;
pub mod running;
pub mod runtime_state;
pub mod shutdown;
