//! # oneshim-api-contracts
//!
//! Shared request/response type contracts between client crates.
//! Ensures API contract consistency across the workspace.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

pub mod ai_providers;
pub mod annotations;
pub mod automation;
pub mod automation_gui;
pub mod backup;
pub mod bug_report;
pub mod coaching;
pub mod common;
pub mod dashboard;
pub mod data;
pub mod digests;
pub mod error;
pub mod events;
pub mod export;
pub mod focus;
pub mod frames;
pub mod idle;
pub mod integration;
pub mod metrics;
pub mod onboarding;
pub mod playbooks;
pub mod pomodoro;
pub mod processes;
pub mod provider_specs;
pub mod recalibration;
pub mod reports;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod stats;
pub mod stream;
pub mod suggestions;
pub mod support;
pub mod tags;
pub mod timeline;
pub mod tracking_schedule;
pub mod update;
