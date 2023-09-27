pub mod args;
pub mod config;

pub mod nma;

use args::parse_args;
use nma::assemble;
use std::process::Command;

fn main() -> Result<(), &'static str> {
    // Parse command line arguments and the config file
    let cmd_args = parse_args()?;

    let config: config::Config = match config::parse_config(&cmd_args) {
        Ok(v) => v,
        _ => {
            println!("WARN : Failed to parse config file, defaulting to nma");
            config::backup_config()
        }
    };

    if config.as_cmd.is_empty() {
        // If no provided as config, default to NMA
        assemble(&cmd_args)?;
    } else {
        // Otherwise, use provided assembler command
        println!("Config Name:   {}", config.config_name);
        println!("Assembler CMD: {:?}", config.as_cmd);

        for cmd in &config.as_cmd {
            match Command::new("sh").arg("-c").arg(cmd.as_str()).output() {
                Ok(output) => {
                    if output.status.success() {
                        if !&output.stdout.is_empty() {
                            println!("CMD {}\n{}", cmd, String::from_utf8_lossy(&output.stdout));
                        }
                    } else if !&output.stderr.is_empty() {
                        eprintln!("CMD {}\n{}", cmd, String::from_utf8_lossy(&output.stderr));
                    }
                }
                Err(err) => {
                    eprintln!("Error: {}", err);
                    return Err("Failed to run assembler command");
                }
            }
        }
    }

    Ok(())
}
