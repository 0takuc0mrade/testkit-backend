//! Assert on contract events without hand-decoding `Val` topics.

mod assertions;
mod capture;

pub use capture::{decode_topic_diagnostic, decode_val_diagnostic, CapturedEvent, EventLog};
