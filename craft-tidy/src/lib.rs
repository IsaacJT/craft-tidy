// SPDX-License-Identifier: GPL-3.0-only

//! craft-tidy, a linter for Snapcraft projects.

pub mod app;
pub mod checks;
pub mod cli;
pub mod config;
pub mod error;
pub mod fix;
pub mod glob;
pub mod project;
pub mod yaml;
