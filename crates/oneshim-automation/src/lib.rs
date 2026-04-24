#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
// P2 PR-C: `missing_const_for_fn` accepted crate-wide. See
// docs/reviews/2026-04-21-p2-missing-const-for-fn-decision.md.
#![allow(clippy::missing_const_for_fn)]

//! # oneshim-automation

pub mod error;
pub use error::AutomationError;

pub mod action_dispatcher;
pub mod audit;
pub mod controller;
pub mod gui_interaction;
pub mod input_driver;
pub mod intent_planner;
pub mod intent_resolver;
pub mod local_llm;
pub mod overlay;
pub mod policy;
pub mod presets;
pub mod resolver;
pub mod sandbox;
