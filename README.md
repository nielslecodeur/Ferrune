# Ferrune

A small proof-of-stake blockchain written in Rust, with its own peer-to-peer network over TCP.

Each node keeps the full chain, exchanges blocks and transactions with its peers, and proposes blocks when it is its turn.

## Features

- **Accounts**: balances, stakes and nonces. Transactions are `Mint`, `Transfer` and `Stake`.
- **Signatures**: transactions and blocks are signed with ed25519. An address is the hex encoded public key.
- **Consensus**: the proposer of each block is picked among the stakers, proportionally to their stake.
- **Network**: handshake with a chain id, peer discovery, liveness with pings, gossip of blocks and transactions, and sync of missing blocks.
- **Storage**: the wallet and the chain are saved on disk and loaded back on start.

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

Logs are controlled with `RUST_LOG`, for example `RUST_LOG=debug`.

### Commands

Once a node runs, type commands in its terminal. Transactions are signed with the node's wallet, and their nonce is set automatically.

| Command | Description |
|---|---|
| `address` | Shows the node's address. |
| `mint <to> <amount>` | Creates coins. Only the mint authority (the root node) can do it. |
| `transfer <to> <amount>` | Sends coins. |
| `stake <amount>` | Stakes coins, which makes the node a validator. |
| `balance [address]` | Shows the balance and the stake of an account (the node's by default). |
| `status` | Shows the height, the number of peers, the mempool size and the next proposer. |

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
    └── producer.rs    Block production
```

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
