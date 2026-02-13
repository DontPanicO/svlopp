// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use std::path::PathBuf;

const DEFAULT_RUN_DIR: &str = "/run/svlopp";

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: PathBuf,
    pub run_dir: PathBuf,
}

fn usage() -> ! {
    eprintln!("usage: svlopp [--run-dir PATH] <config_file>");
    std::process::exit(1);
}

pub fn parse() -> CliArgs {
    let mut args = std::env::args().skip(1);
    let mut config_path = None;
    let mut run_dir = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--run-dir" => {
                run_dir =
                    Some(PathBuf::from(args.next().unwrap_or_else(|| {
                        eprintln!("--run-dir requires a value");
                        usage();
                    })));
            }
            "--help" => usage(),
            other if other.starts_with("-") => {
                eprintln!("unknown option: {}", other);
                usage();
            }
            other => {
                if config_path.is_some() {
                    eprintln!("unexpected argument: {}", other);
                    usage();
                }
                config_path = Some(PathBuf::from(other));
            }
        }
    }
    CliArgs {
        config_path: config_path.unwrap_or_else(|| usage()),
        run_dir: run_dir.unwrap_or_else(|| PathBuf::from(DEFAULT_RUN_DIR)),
    }
}
