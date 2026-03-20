//! Command-line argument parsing for node identity, simulator port, and broadcast port selection.

use crate::config::SYSTEMSTATE_BROADCAST_PORT;

fn print_usage(program: &str) {
    println!("Usage:
        {program} [id] [elevatorserver_port] [broadcast_port]

        Defaults:
        id                     = (auto-generated)
        elevatorserver_port    = 15657
        broadcast_port         = 16659"
    );
}

fn parse_u16_arg(args: &[String], index: usize, default: u16, name: &str) -> u16 {
    match args.get(index) {
        Some(value) => value.parse::<u16>().unwrap_or_else(|_| {
            eprintln!("Invalid {} '{}', expected u16", name, value);
            std::process::exit(2);
        }),
        None => default,
    }
}

/// Parses command-line arguments.
///
/// # Returns
///
/// `(my_id, elevatorserver_port, broadcast_port)` on successful parsing, where:
/// - `my_id`: The elevator identifier as a `String`
/// - `elevatorserver_port`: The simulator port as `u16`
/// - `broadcast_port`: The broadcast port as `u16`
///
/// Returns `None` if the help flag is provided or if argument parsing fails.
///
/// Example: `cargo run -- elev1 15657`
/// ```
pub fn parse() -> (std::string::String, u16, u16) {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_usage(&args[0]);
        std::process::exit(0);
    }

    let elevatorserver_port = parse_u16_arg(&args, 2, 15657, "elevatorserver_port");
    let broadcast_port = parse_u16_arg(&args, 3, SYSTEMSTATE_BROADCAST_PORT, "broadcast_port");

    let my_id = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("elev-{}", elevatorserver_port));

    (my_id, elevatorserver_port, broadcast_port)
}
