// Copyright (c) 2026 Alejandro Gonzales-Irribarren <alejandrxgzi@gmail.com>
// Distributed under the terms of the GNU General Public License v3.0.

//! `netools` command-line entry point. Built only with the `cli` feature.

fn main() -> std::process::ExitCode {
    netools::cli::main()
}
