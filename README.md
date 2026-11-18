# Ferrune

A small proof-of-stake blockchain written in Rust, with its own peer-to-peer network over TCP.

Each node keeps the full chain, exchanges blocks and transactions with its peers, and proposes blocks when it is its turn.

## Features

- **Accounts**: balances, stakes and nonces. Transactions are `Mint`, `Transfer` and `Stake`.
- **Signatures**: transactions and blocks are signed with ed25519. An address is the hex encoded public key.
- **Consensus**: the proposer of each block is picked among the stakers, proportionally to their stake.
- **Network**: handshake with a chain id, peer discovery, liveness with pings, gossip of blocks and transactions, and sync of missing blocks.
- **Storage**: the wallet and the chain are saved on disk and loaded back on start.
- **Explorer**: each node serves a web page with its blocks, accounts, mempool and peers.

## Getting started

```bash
cargo build --release
```

Start a root node. It creates the genesis block and becomes the mint authority:

```bash
cargo run -- --listen 127.0.0.1:8000 --root
```

Start other nodes, each with its own port, and point them to any running node:

```bash
cargo run -- --listen 127.0.0.1:8001 --bootstrap 127.0.0.1:8000
```

```bash
cargo run -- --listen 127.0.0.1:8002 --bootstrap 127.0.0.1:8001
```

### Options

| Option | Default | Description |
|---|---|---|
| `-l, --listen` | `127.0.0.1:8000` | Address to listen on. |
| `-r, --root` | | Starts a new chain instead of joining one. |
| `-b, --bootstrap` | `127.0.0.1:8000` | Peers to connect to on start. Can be repeated. |
| `-c, --chain-id` | `0` | Nodes only connect to nodes with the same chain id. |
| `-d, --data-dir` | `data/<port>` | Where the wallet and the chain are saved. |
| `-w, --web` | `127.0.0.1:<port + 1000>` | Address of the web explorer. |

Logs are controlled with `RUST_LOG`, for example `RUST_LOG=debug`.

### Commands

Once a node runs, type commands in its terminal (or use the explorer). Transactions are signed with the node's wallet, and their nonce is set automatically. A transaction is refused if the address is not valid or if the amount is more than the available coins.

| Command | Description |
|---|---|
| `address` | Shows the node's address. |
| `mint <to> <amount>` | Creates coins. Only the mint authority (the root node) can do it. |
| `transfer <to> <amount>` | Sends coins. |
| `stake <amount>` | Stakes coins, which makes the node a validator. |
| `balance [address]` | Shows the balance and the stake of an account (the node's by default). |
| `status` | Shows the height, the number of peers, the mempool size and the next proposer. |

### Explorer

Open `http://127.0.0.1:9000` for a node listening on port `8000` (the port plus 1000). The page refreshes every 2 seconds and shows:

- the height, the number of peers, the mempool size and the next proposer,
- every block, with its hash, its proposer and its transactions (click a block to open it),
- the accounts with their balance and stake, the pending transactions and the peers.

Click an address to copy it. The node's own address is tagged `you`, and the mint authority `mint`. The raw data is available as JSON on `/api`.

The **Your wallet** panel shows the node's balance, stake and available coins (the balance minus what is already in the mempool), and sends transactions signed with the node's wallet:

- **Transfer**: sends coins to an address. Click **Send** on an account to fill in its address.
- **Stake**: stakes coins to become a validator.
- **Mint**: creates coins. Only shown on the root node.

By default, the explorer is only reachable from the same computer. Use `--web 0.0.0.0:9000` to open it to the network: other computers can then look at the page, but only the computer running the node can send transactions.

### Example

1. On the second node, run `address` and copy the address.
2. On the root node, run `mint <address> 100`. The root node includes it in the next block.
3. On the second node, run `balance`, then `stake 20`.
4. Once the stake is in a block, the second node is the only validator and proposes the next blocks.

## How it works

### Chain

- The root node creates the genesis block. Its proposer becomes the mint authority.
- A block is valid if its height and previous hash follow the last block, its hash is right, it is signed by its proposer, the proposer is the expected one, and all its transactions apply. Otherwise, nothing is applied.
- Every 5 seconds, the expected proposer builds a block from the valid transactions of its mempool (at most 100).

### Consensus

The proposer of the next block is picked among the accounts with a stake, with a probability proportional to the stake. The hash of the previous block is the random seed, so every node picks the same proposer. Until someone stakes, the mint authority proposes every block.

### Network

Nodes talk with newline-delimited JSON messages over TCP. Each message has an id and belongs to a conversation, and a node checks that each answer is the one expected at this step:

| Conversation | Messages |
|---|---|
| Connection | `LetsConnect` → `ChainId` → `Register` → `Ok` / `Reject` |
| Discovery | `AskPeerAddr` → `PeerAddress` / `NoPeerFound` |
| Block | `SendBlock` → `Ok` / `Reject` |
| Transaction | `SendTransaction` → `Ok` / `Reject` |
| Sync | `AskHeight` → `Height`, then `AskBlocks` → `Blocks` |
| Liveness | `Ping` → `Pong` |
| Leaving | `Disconnect` |

- Once connected, both nodes ask each other's height and download the missing blocks, by batches of 100.
- New blocks and transactions are sent to every other peer. Known ones are not sent again.
- Every 60 seconds, a node asks a peer for another peer's address, up to 8 peers.
- Peers are pinged every 10 seconds, and dropped after 30 seconds without any message.

### Storage

Each node saves in its data directory:

- `wallet.key`: the secret key, created on the first start.
- `chain.jsonl`: the blocks, one per line. They are checked again when loaded.

## Project structure

```
src
├── main.rs            Command line and node start
├── consts.rs          Settings (intervals, limits)
├── blockchain         Blocks, transactions, state, mempool, wallet, storage
├── network            Connections, messages and conversation states
└── node
    ├── actions        What a node does when it receives a message
    ├── cli.rs         Commands typed in the terminal
    ├── explorer.rs    Web explorer (explorer.html)
    └── producer.rs    Block production
```

## Profiling

CPU flamegraphs of two nodes on localhost, over 30 seconds: a root node sending 3,000 transfers, and a node receiving them. Open them in a browser to zoom and search.

| | Producer | Receiver |
|---|---|---|
| Before | [before-producer.svg](docs/profiling/before-producer.svg) | [before-receiver.svg](docs/profiling/before-receiver.svg) |
| After | [after-producer.svg](docs/profiling/after-producer.svg) | [after-receiver.svg](docs/profiling/after-receiver.svg) |

Before, two system calls were made for every message:

- **Producer**: `Uuid::new_v4` asked the system for random bytes for every message id. The `fast-rng` feature of `uuid` now uses a fast generator seeded once.
- **Receiver**: every received transaction was written to the `INFO` log. These logs are now at `DEBUG` level (`RUST_LOG=debug` to see them).

Measured without the profiler (user + system CPU time, same scenario, 3 runs each):

| | Before | After |
|---|---|---|
| Producer | 0.485 s | 0.45 s (-7%) |
| Receiver | 0.30 s | 0.27 s (-10%) |

The flamegraphs overstate these two calls (68% and 78%): on macOS, the profiling signal is delivered when a system call returns, so the time spent in the kernel is charged to the system call that was running. In the "after" flamegraphs, the same time is charged to the TCP write of each message. Signature checks and hashing take about 3%, and the nodes are mostly idle (3,000 transfers are accepted in 0.4 s).

They were recorded with [pprof](https://crates.io/crates/pprof) (`flamegraph` and `frame-pointer` features), on a release build with debug symbols and frame pointers.

## Tests

```bash
cargo test
```

The network tests start real nodes on localhost, on ports 47100 to 47142.

## Limitations

- If the expected proposer is offline, no block is produced until it comes back.
- Forks are not handled: a node keeps the first valid block it receives for each height.
- Messages are not encrypted.

## License

MIT, see [LICENSE](LICENSE).
