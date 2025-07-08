use clap::Command;

pub struct DaemonCommandBuilder {
    command: Command,
    subcommands: Vec<Command>,
}

impl DaemonCommandBuilder {
    pub fn new() -> Self {
        DaemonCommandBuilder {
            command: Command::new("daemon")
                .about("Manage the BrokerSync daemon process")
                .arg_required_else_help(true),
            subcommands: Vec::new(),
        }
    }

    pub fn build(self) -> Command {
        self.command.subcommands(self.subcommands)
    }

    pub fn start(mut self) -> Self {
        self.subcommands.push(
            Command::new("start")
                .about("Start the BrokerSync daemon for real-time order synchronization")
                .long_about(
                    "Start the BrokerSync daemon process that maintains a persistent WebSocket \
                    connection to Alpaca for real-time order updates. The daemon runs in the \
                    background and automatically syncs order status changes to the local database.",
                ),
        );
        self
    }

    pub fn stop(mut self) -> Self {
        self.subcommands.push(
            Command::new("stop")
                .about("Stop the BrokerSync daemon")
                .long_about(
                    "Gracefully stop the BrokerSync daemon process. This will close the WebSocket \
                    connection and stop real-time synchronization.",
                ),
        );
        self
    }

    pub fn status(mut self) -> Self {
        self.subcommands.push(
            Command::new("status")
                .about("Show the current status of the BrokerSync daemon")
                .long_about(
                    "Display information about the BrokerSync daemon including connection status, \
                    uptime, active accounts, and last synchronization time.",
                ),
        );
        self
    }

    pub fn restart(mut self) -> Self {
        self.subcommands.push(
            Command::new("restart")
                .about("Restart the BrokerSync daemon")
                .long_about(
                    "Stop the current daemon process (if running) and start a new one. \
                    This is equivalent to running 'daemon stop' followed by 'daemon start'.",
                ),
        );
        self
    }
}
