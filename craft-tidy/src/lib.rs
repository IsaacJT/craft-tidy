// SPDX-License-Identifier: GPL-3.0-only

//! craft-tidy, a linter for the project files of Canonical's craft tools.

pub mod app;
pub mod checks;
pub mod cli;
pub mod config;
pub mod craft;
pub mod error;
pub mod fix;
pub mod glob;
pub mod nolint;
pub mod project;
pub mod yaml;
