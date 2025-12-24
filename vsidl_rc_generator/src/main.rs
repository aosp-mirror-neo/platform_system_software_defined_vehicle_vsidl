// Copyright 2025 Google LLC
//! VSIDL runtime config generator
//! Reads and parses a VSIDL catalog and its dependencies. Outputs runtime configuration files used by SDV agents

use clap::Parser;

fn main() -> anyhow::Result<()> {
    vsidl_rc_generator::execute(vsidl_rc_generator::Cli::parse())
}
