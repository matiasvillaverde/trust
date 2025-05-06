# Trust: Algorithmic Trading Tool

Welcome to Trust Tool written in Rust, currently in its beta phase. Offering risk management features such as controlling maximum risk per trade and maximum risk per month. It crafts trades with a precise stop, an entry, and a target. Furthermore, while it currently works seamlessly with the [Alpaca API](https://alpaca.markets/), it is built with extensibility in mind, facilitating the addition of different brokers.

Please note: This product is in beta, and you should proceed only if you comprehend the underlying code and workings.

Here you can find documentation about the project: https://deepwiki.com/matiasvillaverde/trust

## Features

- Maximum risk per trade.
- Maximum risk per month.
- Constructs trades with a stop, an entry, and a target.
- Interoperability with Alpaca API.
- Flexibility to manually submit the orders to your favorite broker.

## Prerequisites

Make sure you have Rust installed.

## Installation

Clone this repository:

``` bash
git clone https://github.com/matiasvillaverde/trust.git
cd trust
```

## Usage

To start using the tool, you can run it as specified below:

``` bash
# Explore the available commands
cargo run --bin cli  -- help

# Create an account
cargo run --bin cli  -- account create

# Create a risk rule, like maximum risk per trade and maximum risk per month
cargo run --bin cli  -- rule create

# Add funds to the account
cargo run --bin cli  -- transaction deposit

# Create a symbol
cargo run --bin cli  -- trading-vehicle create

# Add Alpaca API keys
cargo run --bin cli  -- key create

# Create a trade
cargo run --bin cli  -- trade create

# Fund the trade and pass all the risk checks
cargo run --bin cli  -- trade fund

# Submit the trade to Alpaca
cargo run --bin cli  -- trade submit

# Explore more commands
cargo run --bin cli  -- [command] help

```

## Disclaimer

This tool is currently in the beta phase and should be used cautiously. You should only proceed if you understand how the underlying code operates. There might be bugs and unexpected behavior on rare occasions.

## License

MIT License - see the LICENSE file for details.

## Support

If you encounter any problems, please open an issue. We'll try to resolve it as soon as possible.
