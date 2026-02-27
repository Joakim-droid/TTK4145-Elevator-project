use crate::config::PEER_DISCOVERY_BCAST_PORT;

fn print_usage(program: &str) {
    println!(
        "Usage:
  {program} [id] [sim_port] [bcast_port]

Defaults:
  id            = (auto-generated)
  sim_port      = 15658
  bcast_port    = 16659"
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
/// `(my_id, sim_port, bcast_port)` on successful parsing, where:
/// - `my_id`: The elevator identifier as a `String`
/// - `sim_port`: The simulator port as `u16`
/// - `bcast_port`: The broadcast port as `u16`
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

    let sim_port = parse_u16_arg(&args, 2, 15658, "sim_port");
    let bcast_port = parse_u16_arg(&args, 3, PEER_DISCOVERY_BCAST_PORT, "bcast_port");

    let my_id = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| format!("elev-{}", sim_port));

    (my_id, sim_port, bcast_port)
}
