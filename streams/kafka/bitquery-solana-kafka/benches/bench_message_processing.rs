use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn generate_test_trades(_n: usize) -> Vec<DummyTrade> {
    vec![DummyTrade; 1000]
}

async fn process_trade(_trade: &DummyTrade) {}

#[derive(Clone)]
struct DummyTrade;

fn benchmark_message_processing(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    c.bench_function("process_1000_trades", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let trades = generate_test_trades(1000);
                for trade in trades {
                    black_box(process_trade(&trade).await);
                }
            });
        });
    });
}

criterion_group!(benches, benchmark_message_processing);
criterion_main!(benches);
