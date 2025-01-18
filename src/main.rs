use std::io;
use std::io::BufRead;
use std::io::Write;
use std::process;

use clap::ValueEnum;
use clap::{Args, Parser, Subcommand};
use resistance_civil_protection::email;
use resistance_civil_protection::CivilProtection;
use syslog::BasicLogger;
use syslog::Facility;
use syslog::Formatter3164;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    squadmate_cmd: SquadmateCommands,
}

#[derive(Subcommand, Debug)]
enum SquadmateCommands {
    Setup,
    Add(SquadmateAddArgs),
    Remove(SquadmateRmArgs),
}

#[derive(Args, Debug)]
struct SquadmateAddArgs {
    name: String,
    email: String,
}

#[derive(Args, Debug)]
struct SquadmateRmArgs {
    #[arg(value_enum)]
    field_type: SquadmateRmFieldType,
    value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum SquadmateRmFieldType {
    Email,
    Name,
}

fn check_config(cp: &CivilProtection) {
    if !cp.is_config_loaded() {
        eprintln!("Resistance is not setup yet!");
        process::exit(1);
    }
}

fn setup_logging() {
    // Setup system logging
    let formatter = Formatter3164 {
        facility: Facility::LOG_DAEMON,
        hostname: None,
        process: "resistance-cmacm".into(),
        pid: process::id(),
    };

    let syslog_logger = match syslog::unix(formatter) {
        Ok(logger) => logger,
        Err(_) => {
            return;
        }
    };

    let syslog_box = Box::new(BasicLogger::new(syslog_logger));
    let _ = log::set_boxed_logger(syslog_box).map(|()| log::set_max_level(log::LevelFilter::Info));
}

fn main() {
    setup_logging();

    let cli = Cli::parse();
    let mut cp = CivilProtection::new();

    match &cli.squadmate_cmd {
        SquadmateCommands::Setup => {
            let mut stdout = io::stdout().lock();
            let mut stdin = io::stdin().lock();

            if cp.is_config_loaded() {
                let mut buf = String::new();
                print!("Resistance is already setup! Are you sure you want to reinitialize setup? (y/N): ");
                stdout.flush().unwrap();

                stdin.read_line(&mut buf).unwrap_or_else(|e| {
                    eprintln!("Failed to read from standard input: {}", e);
                    process::exit(1);
                });

                if !buf.to_lowercase().starts_with("y") {
                    println!("Canceled");
                    process::exit(1);
                } else {
                    cp.delete_config().unwrap_or_else(|e| {
                        eprintln!("Failed to delete existing config: {}", e);
                        process::exit(1);
                    });
                }
            }

            let mut email_name = String::new();
            print!("Enter a human-readable name to show to recipients when sending emails: ");
            stdout.flush().unwrap();
            stdin.read_line(&mut email_name).unwrap_or_else(|e| {
                eprintln!("Failed to read from standard input: {}", e);
                process::exit(1);
            });

            let mut email_address = String::new();
            print!("Enter the email address to send emails from: ");
            stdout.flush().unwrap();
            stdin.read_line(&mut email_address).unwrap_or_else(|e| {
                eprintln!("Failed to read from standard input: {}", e);
                process::exit(1);
            });

            let email_password = rpassword::prompt_password(
                "Enter the password for the email address given above: ",
            )
            .unwrap_or_else(|e| {
                eprintln!("Failed to read password: {}", e);
                process::exit(1);
            });

            cp.create_config(
                email::Identity {
                    name: email_name,
                    email: email_address,
                },
                email_password,
            )
            .unwrap_or_else(|e| {
                eprintln!("Failed to setup Resistance: {}", e);
                process::exit(1);
            });

            println!("Logging in...");
            cp.login().unwrap_or_else(|e| {
                eprintln!("Failed to login: {}", e);
                process::exit(1);
            });

            cp.save_config().unwrap_or_else(|e| {
                eprintln!("Failed to setup Resistance: {}", e);
                process::exit(1);
            });

            println!("Resistance has been successfully setup")
        }
        SquadmateCommands::Add(args) => {
            check_config(&cp);

            cp.add_squadmate(email::Identity {
                name: args.name.clone(),
                email: args.email.clone(),
            })
            .unwrap_or_else(|e| {
                eprintln!("Failed to add squadmate: {}", e);
                process::exit(1);
            });

            cp.save_config().unwrap_or_else(|e| {
                eprintln!("Failed to add squadmate: {}", e);
                process::exit(1);
            });

            println!("Successfully added squadmate");
        }
        SquadmateCommands::Remove(args) => {
            check_config(&cp);

            match args.field_type {
                SquadmateRmFieldType::Name => {
                    let squadmate = cp
                        .find_squadmate_by_name(args.value.as_str())
                        .unwrap_or_else(|e| {
                            eprintln!("Error trying to find squadmate with name {}: {}", args.value, e);
                            process::exit(1);
                        })
                        .unwrap_or_else(|| {
                            eprintln!("Unable to find squadmate with name {}", args.value);
                            process::exit(1);
                        })
                        .clone();

                    cp.rm_squadmate(&squadmate).unwrap_or_else(|e| {
                        eprintln!("Failed to remove squadmate: {}", e);
                        process::exit(1);
                    });

                    cp.save_config().unwrap_or_else(|e| {
                        eprintln!("Failed to save config: {}", e);
                        process::exit(1);
                    });

                    println!("Successfully removed squadmate {} <{}>", squadmate.name, squadmate.email);
                },
                SquadmateRmFieldType::Email => {
                    let squadmate = cp
                        .find_squadmate_by_email(args.value.as_str())
                        .unwrap_or_else(|e| {
                            eprintln!("Error trying to find squadmate with email {}: {}", args.value, e);
                            process::exit(1);
                        })
                        .unwrap_or_else(|| {
                            eprintln!("Unable to find squadmate with email {}", args.value);
                            process::exit(1);
                        })
                        .clone();

                    cp.rm_squadmate(&squadmate).unwrap_or_else(|e| {
                        eprintln!("Failed to remove squadmate: {}", e);
                        process::exit(1);
                    });

                    cp.save_config().unwrap_or_else(|e| {
                        eprintln!("Failed to save config: {}", e);
                        process::exit(1);
                    });

                    println!("Successfully removed squadmate {} <{}>", squadmate.name, squadmate.email);
                },
            }
        }
    }
}
