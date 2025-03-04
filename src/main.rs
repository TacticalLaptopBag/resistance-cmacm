use std::io;
use std::io::BufRead;
use std::io::StdinLock;
use std::io::StdoutLock;
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
    squadmate_cmd: Option<SquadmateCommands>,
}

#[derive(Subcommand, Debug)]
enum SquadmateCommands {
    Setup(SquadmateSetupArgs),
    Add(SquadmateAddArgs),
    Remove(SquadmateRmArgs),
    Test,
}

#[derive(Args, Debug)]
struct SquadmateSetupArgs {
    #[arg(value_enum)]
    email_method: Option<SquadmateSetupEmailMethod>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum SquadmateSetupEmailMethod {
    Smtp,
    Sendmail,
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
    if !cp.does_config_exist() {
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

fn prompt_yn(prompt: &str, stdout: &mut StdoutLock, stdin: &mut StdinLock) -> bool {
    let mut buf = String::new();
    print!("{} (y/N): ", prompt);
    stdout.flush().unwrap();

    stdin.read_line(&mut buf).unwrap_or_else(|e| {
        eprintln!("Failed to read from standard input: {}", e);
        process::exit(1);
    });

    if !buf.to_lowercase().starts_with("y") {
        return false
    }
    true
}

fn cmd_setup_check(cp: &mut CivilProtection, stdout: &mut StdoutLock, stdin: &mut StdinLock) {
    if cp.does_config_exist() {
        let response_yes = prompt_yn("Resistance is already setup! Are you sure you want to reinitialize setup?", stdout, stdin);
        if response_yes {
            cp.delete_config().unwrap_or_else(|e| {
                eprintln!("Failed to delete existing config: {}", e);
                process::exit(1);
            });
        } else {
            println!("Canceled");
            process::exit(1);
        }
    }
}

fn cmd_setup_prompt_identity(stdout: &mut StdoutLock, stdin: &mut StdinLock) -> email::Identity {
    let mut email_name = String::new();
    print!("Enter a human-readable name to show to recipients when sending emails: ");
    stdout.flush().unwrap();
    stdin.read_line(&mut email_name).unwrap_or_else(|e| {
        eprintln!("Failed to read from standard input: {}", e);
        process::exit(1);
    });
    email_name = email_name.trim_end().to_string();

    let mut email_address = String::new();
    print!("Enter the email address to send emails from: ");
    stdout.flush().unwrap();
    stdin.read_line(&mut email_address).unwrap_or_else(|e| {
        eprintln!("Failed to read from standard input: {}", e);
        process::exit(1);
    });
    email_address = email_address.trim_end().to_string();

    email::Identity {
        name: email_name,
        email: email_address,
    }
}

fn cmd_setup_confirm_config(cp: &mut CivilProtection, create_config_result: Result<(), Box<dyn std::error::Error>>) {
    create_config_result.unwrap_or_else(|e| {
        eprintln!("Failed to setup Resistance: {}", e);
        process::exit(1);
    });

    println!("Logging in...");
    cp.login().unwrap_or_else(|e| {
        eprintln!("Failed to login: {}", e);
        process::exit(1);
    });

    println!("Resistance has been successfully setup")
}

fn cmd_setup_smtp(cp: &mut CivilProtection) {
    let mut stdout = io::stdout().lock();
    let mut stdin = io::stdin().lock();

    cmd_setup_check(cp, &mut stdout, &mut stdin);

    let identity = cmd_setup_prompt_identity(&mut stdout, &mut stdin);

    let email_password = rpassword::prompt_password(
        "Enter the password for the email address given above: ",
    )
    .unwrap_or_else(|e| {
        eprintln!("Failed to read password: {}", e);
        process::exit(1);
    });

    let result = cp.create_config_smtp(
        identity,
        email_password,
    );

    cmd_setup_confirm_config(cp, result);
}

fn cmd_setup_sendmail(cp: &mut CivilProtection) {
    let mut stdout = io::stdout().lock();
    let mut stdin = io::stdin().lock();

    cmd_setup_check(cp, &mut stdout, &mut stdin);

    let identity = cmd_setup_prompt_identity(&mut stdout, &mut stdin);
    let result = cp.create_config_sendmail(identity);
    cmd_setup_confirm_config(cp, result);
}

fn cmd_setup(cp: &mut CivilProtection, args: &SquadmateSetupArgs) {
    match args.email_method {
        Some(email_method) => {
            match email_method {
                SquadmateSetupEmailMethod::Smtp => cmd_setup_smtp(cp),
                SquadmateSetupEmailMethod::Sendmail => cmd_setup_sendmail(cp),
            }
        }
        None => {
            let conf = cp.config().unwrap_or_else(|_| {
                eprintln!("Not configured yet! Run with `--help` to show setup commands");
                process::exit(1);
            });

            println!("Transport: {}", conf.email_setting);
            println!("From Address: {}", conf.email);
            if conf.squadmates.is_empty() {
                println!("No squadmates! Add some with `cmacm add \"John Doe\" johndoe@example.com`");
            } else {
                println!("Squadmates:");
                for squadmate in conf.squadmates {
                    println!("\t{}", squadmate);
                }
            }
        },
    }
}

fn cmd_add(cp: &mut CivilProtection, args: &SquadmateAddArgs) {
    check_config(&cp);

    let squadmate = email::Identity {
        name: args.name.clone(),
        email: args.email.clone(),
    };

    cp.add_squadmate(squadmate.clone())
        .unwrap_or_else(|e| {
            eprintln!("Failed to add squadmate: {}", e);
            process::exit(1);
        });

    println!("Successfully added squadmate: {}", squadmate);
}

fn cmd_remove(cp: &mut CivilProtection, args: &SquadmateRmArgs) {
    check_config(&cp);

    let squadmate = match args.field_type {
        SquadmateRmFieldType::Name => {
            cp.find_squadmate_by_name(args.value.as_str())
                .unwrap_or_else(|e| {
                    eprintln!("Error trying to find squadmate with name {}: {}", args.value, e);
                    process::exit(1);
                })
                .unwrap_or_else(|| {
                    eprintln!("Unable to find squadmate with name {}", args.value);
                    process::exit(1);
                })
        },
        SquadmateRmFieldType::Email => {
            cp.find_squadmate_by_email(args.value.as_str())
                .unwrap_or_else(|e| {
                    eprintln!("Error trying to find squadmate with email {}: {}", args.value, e);
                    process::exit(1);
                })
                .unwrap_or_else(|| {
                    eprintln!("Unable to find squadmate with email {}", args.value);
                    process::exit(1);
                })
        },
    };

    let mut stdout = io::stdout().lock();
    let mut stdin = io::stdin().lock();

    let response_yes = prompt_yn(
        format!("Found squadmate {}, are you sure you want to remove them?", squadmate).as_str(),
        &mut stdout,
        &mut stdin,
    );

    if response_yes {
        cp.rm_squadmate(&squadmate).unwrap_or_else(|e| {
            eprintln!("Failed to remove squadmate: {}", e);
            process::exit(1);
        });

        println!("Successfully removed squadmate {}", squadmate);
    } else {
        println!("Canceled");
    }
}

fn cmd_test(cp: &mut CivilProtection) {
    cp.notify_squadmates().unwrap_or_else(|e| {
        eprintln!("Failed to send email! Is Resistance setup correctly?");
        eprintln!("{}", e);
        process::exit(1);
    });

    println!("Sent a test email to all Squadmates. Confirm with them that they received the email.");
}

fn main() {
    setup_logging();

    let cli = Cli::parse();
    let mut cp = CivilProtection::new();

    match &cli.squadmate_cmd {
        Some(cmd) => {
            match &cmd {
                SquadmateCommands::Setup(args) => cmd_setup(&mut cp, args),
                SquadmateCommands::Add(args) => cmd_add(&mut cp, args),
                SquadmateCommands::Remove(args) => cmd_remove(&mut cp, args),
                SquadmateCommands::Test => cmd_test(&mut cp),
            }
        },
        None => cmd_setup(&mut cp, &SquadmateSetupArgs { email_method: None })
    }

}
