#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

//! # oneshim-suggestion

pub mod error;
pub use error::SuggestionError;

pub mod feedback;
pub mod history;
pub mod presenter;
pub mod queue;
pub mod receiver;
