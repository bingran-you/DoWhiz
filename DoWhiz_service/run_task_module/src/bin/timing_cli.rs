use run_task_module::{clear_timing_log, get_timing_log_path};
use std::env;
use std::process::Command;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_help();
        return;
    }

    match args[1].as_str() {
        "clear" => {
            let path = get_timing_log_path();
            match clear_timing_log() {
                Ok(()) => {
                    println!("Cleared timing log: {}", path.display());
                }
                Err(e) => {
                    eprintln!("Failed to clear timing log: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "path" => {
            println!("{}", get_timing_log_path().display());
        }
        "show" => {
            let path = get_timing_log_path();
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => print!("{}", content),
                    Err(e) => {
                        eprintln!("Failed to read timing log: {}", e);
                        std::process::exit(1);
                    }
                }
            } else {
                println!("No timing log found at {}", path.display());
            }
        }
        "analyze" => {
            let path = get_timing_log_path();
            if !path.exists() {
                eprintln!("No timing log found at {}", path.display());
                std::process::exit(1);
            }

            // Find the analyze_timings.py script relative to the binary or via env
            let script_path = env::var("ANALYZE_SCRIPT_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    // Try common locations
                    let candidates = [
                        "scripts/analyze_timings.py",
                        "DoWhiz_service/scripts/analyze_timings.py",
                        "../scripts/analyze_timings.py",
                    ];
                    for candidate in candidates {
                        let p = std::path::PathBuf::from(candidate);
                        if p.exists() {
                            return p;
                        }
                    }
                    std::path::PathBuf::from("scripts/analyze_timings.py")
                });

            println!(
                "Running: python {} {}",
                script_path.display(),
                path.display()
            );

            let status = Command::new("python").arg(&script_path).arg(&path).status();

            match status {
                Ok(s) if s.success() => {}
                Ok(s) => {
                    eprintln!("Analysis script exited with: {}", s);
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("Failed to run analysis script: {}", e);
                    std::process::exit(1);
                }
            }
        }
        "--help" | "-h" | "help" => {
            print_help();
        }
        cmd => {
            eprintln!("Unknown command: {}", cmd);
            print_help();
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        r#"timing_cli - Manage task timing logs

USAGE:
    timing_cli <COMMAND>

COMMANDS:
    clear    Clear the timing log file
    path     Print the path to the timing log file
    show     Display the contents of the timing log
    analyze  Run the Python analysis script to generate plots
    help     Print this help message

ENVIRONMENT:
    TIMING_LOG_PATH       Override default timing log path (default: ./task_timings.jsonl)
    ANALYZE_SCRIPT_PATH   Override path to analyze_timings.py script
"#
    );
}
