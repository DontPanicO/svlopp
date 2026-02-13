// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

const DEFAULT_CONTROL_PATH: &str = "/run/svlopp/control";

#[derive(Debug, Clone)]
pub struct CliArgs {
    pub config_path: String,
    pub control_path: String,
}

fn usage() -> ! {
    eprintln!("usage: svlopp [--control-path PATH] <config_file>");
    std::process::exit(1);
}

pub fn parse() -> CliArgs {
    let mut args = std::env::args().skip(1);
    let mut config_path = None;
    let mut control_path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--control-path" => {
                control_path = Some(args.next().unwrap_or_else(|| {
                    eprintln!("--control-path requires a value");
                    usage();
                }));
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
                config_path = Some(other.to_string());
            }
        }
    }
    CliArgs {
        config_path: config_path.unwrap_or_else(|| usage()),
        control_path: control_path
            .unwrap_or_else(|| DEFAULT_CONTROL_PATH.to_owned()),
    }
}
