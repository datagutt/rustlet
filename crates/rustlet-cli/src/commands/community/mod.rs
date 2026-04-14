//! `rustlet community` subcommands. Parity with pixlet's `community` group:
//! thin utilities for app authors submitting to the Tronbyt community.

use std::path::PathBuf;

use anyhow::Result;
use clap::Subcommand;

pub mod list_color_filters;
pub mod list_fonts;
pub mod list_icons;
pub mod load_app;
pub mod validate_icons;
pub mod validate_manifest;

#[derive(Subcommand)]
pub enum CommunityAction {
    /// Validate the manifest.yaml next to an applet.
    ValidateManifest {
        /// Path to an applet directory (defaults to cwd).
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Soft-check every schema field's icon (non-empty, ASCII identifier).
    ValidateIcons {
        /// Path to an applet file or directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// List every registered color filter.
    ListColorFilters,
    /// List every embedded bitmap font with its metrics.
    ListFonts,
    /// Print guidance on icon usage.
    ListIcons,
    /// Load and evaluate an applet with an empty config.
    LoadApp {
        /// Path to an applet file or directory.
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Interactive manifest scaffolder. Alias for `rustlet create`.
    CreateManifest,
}

pub fn run(action: CommunityAction) -> Result<()> {
    match action {
        CommunityAction::ValidateManifest { path } => validate_manifest::run(&path),
        CommunityAction::ValidateIcons { path } => validate_icons::run(&path),
        CommunityAction::ListColorFilters => list_color_filters::run(),
        CommunityAction::ListFonts => list_fonts::run(),
        CommunityAction::ListIcons => list_icons::run(),
        CommunityAction::LoadApp { path } => load_app::run(&path),
        CommunityAction::CreateManifest => crate::commands::create::run(),
    }
}
