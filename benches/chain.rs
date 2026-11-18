//! Benchmarks of the CPU-heavy parts of a node: signatures, hashing, the
//! mempool, building and applying blocks, picking the proposer.
//!
//! Run with `cargo bench`. The HTML report is in `target/criterion/report`.

use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use ferrune::blockchain::block::Block;
use ferrune::blockchain::core::Blockchain;
use ferrune::blockchain::mempool::Mempool;
use ferrune::blockchain::state::{Account, State};
use ferrune::blockchain::transaction::{Address, SignedTransaction, Transaction};
use ferrune::blockchain::wallet::Wallet;
use ferrune::consts::MAX_TRANSACTIONS_PER_BLOCK;

/// Chain whose genesis was proposed by `admin`, with a block minting coins to it.
/// `admin` is the mint authority and the proposer of the next blocks.
fn funded_chain(admin: &Wallet) -> Blockchain {
    let mut chain = Blockchain::new(State::new(Address(String::new())));
    chain
        .add_block(Blockchain::genesis(admin).unwrap())
        .unwrap();

    let mint = admin
        .sign_transaction(Transaction::Mint {
            to: admin.address(),
            amount: 1_000_000_000,
            nonce: 0,
        })
        .unwrap();
    chain.add_transaction(mint).unwrap();

    let block = chain.build_block(admin).unwrap().unwrap();
    chain.add_block(block).unwrap();
    chain
}

/// `count` signed transfers from `admin`, with the nonces that follow the mint.
fn transfers(admin: &Wallet, count: usize) -> Vec<SignedTransaction> {
    let recipient = Wallet::generate().address();

    (0..count as u64)
        .map(|index| {
            admin
                .sign_transaction(Transaction::Transfer {
                    from: admin.address(),
                    to: recipient.clone(),
                    amount: 1,
                    nonce: index + 1,
                })
                .unwrap()
        })
        .collect()
}

fn chain_with_mempool(admin: &Wallet, count: usize) -> Blockchain {
    let mut chain = funded_chain(admin);
    for transaction in transfers(admin, count) {
        chain.add_transaction(transaction).unwrap();
    }
    chain
}

fn crypto(c: &mut Criterion) {
    let wallet = Wallet::generate();
    let transaction = Transaction::Transfer {
        from: wallet.address(),
        to: Wallet::generate().address(),
        amount: 10,
        nonce: 0,
    };
    let signed = wallet.sign_transaction(transaction.clone()).unwrap();

    let mut group = c.benchmark_group("crypto");
    group.bench_function("sign transaction", |b| {
        b.iter(|| {
            wallet
                .sign_transaction(black_box(transaction.clone()))
                .unwrap()
        })
    });
    group.bench_function("verify transaction", |b| {
        b.iter(|| black_box(&signed).has_valid_signature())
    });

    let admin = Wallet::generate();
    let block = Block::new(
        1,
        [0; 32],
        transfers(&admin, MAX_TRANSACTIONS_PER_BLOCK),
        admin.address(),
    )
    .unwrap();
    group.throughput(Throughput::Elements(MAX_TRANSACTIONS_PER_BLOCK as u64));
    group.bench_function("hash block (100 tx)", |b| {
        b.iter(|| black_box(&block).calculate_hash().unwrap())
    });
    group.finish();
}

fn mempool(c: &mut Criterion) {
    let admin = Wallet::generate();
    let pending = transfers(&admin, MAX_TRANSACTIONS_PER_BLOCK);

    let mut group = c.benchmark_group("mempool");
    group.throughput(Throughput::Elements(pending.len() as u64));
    group.bench_function("insert 100 tx", |b| {
        b.iter_batched(
            || pending.clone(),
            |pending| {
                let mut mempool = Mempool::default();
                for transaction in pending {
                    mempool.insert(transaction).unwrap();
                }
                mempool
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();

    // Computing the next nonce goes through the whole mempool.
    let mut group = c.benchmark_group("next nonce");
    for size in [10, 100, 1_000] {
        let chain = chain_with_mempool(&admin, size);
        let next = Transaction::Transfer {
            from: admin.address(),
            to: Wallet::generate().address(),
            amount: 1,
            nonce: 0,
        };

        group.bench_with_input(BenchmarkId::from_parameter(size), &chain, |b, chain| {
            b.iter(|| chain.next_nonce(black_box(&next)))
        });
    }
    group.finish();
}

fn blocks(c: &mut Criterion) {
    let admin = Wallet::generate();
    let chain = chain_with_mempool(&admin, MAX_TRANSACTIONS_PER_BLOCK);
    let block = chain.build_block(&admin).unwrap().unwrap();

    let mut group = c.benchmark_group("block");
    group.throughput(Throughput::Elements(MAX_TRANSACTIONS_PER_BLOCK as u64));
    group.sample_size(30);

    group.bench_function("build (100 tx)", |b| {
        b.iter(|| chain.build_block(black_box(&admin)).unwrap())
    });
    group.bench_function("add (100 tx)", |b| {
        b.iter_batched(
            || (chain.clone(), block.clone()),
            |(mut chain, block)| chain.add_block(block).unwrap(),
            BatchSize::LargeInput,
        )
    });

    let json = serde_json::to_vec(&block).unwrap();
    group.bench_function("serialize (100 tx)", |b| {
        b.iter(|| serde_json::to_vec(black_box(&block)).unwrap())
    });
    group.bench_function("deserialize (100 tx)", |b| {
        b.iter(|| serde_json::from_slice::<Block>(black_box(&json)).unwrap())
    });
    group.finish();
}

fn proposer(c: &mut Criterion) {
    let admin = Wallet::generate();
    let mut group = c.benchmark_group("proposer");

    for stakers in [10, 100, 1_000] {
        let mut chain = funded_chain(&admin);
        for index in 0..stakers {
            chain.state.accounts.insert(
                Address(format!("{index:064x}")),
                Account {
                    balance: 0,
                    stake: 1 + index as u64,
                    nonce: 0,
                },
            );
        }

        group.bench_with_input(BenchmarkId::from_parameter(stakers), &chain, |b, chain| {
            b.iter(|| chain.proposer())
        });
    }
    group.finish();
}

criterion_group!(benches, crypto, mempool, blocks, proposer);
criterion_main!(benches);
