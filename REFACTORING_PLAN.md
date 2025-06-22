# Zolca-Ops Refactoring Plan

_A comprehensive roadmap to improve architecture, modularity, observability, and testability across the Zolca-Ops workspace._

---

## Table of Contents

- [1. Goals](#1-goals)  
- [2. Architectural Refactoring](#2-architectural-refactoring)  
  - [2.1. Clean/Hexagonal Layers](#21-cleanhexagonal-layers)  
  - [2.2. Dependency Injection & Composition](#22-dependency-injection--composition)  
  - [2.3. Plugin/Strategy Registry](#23-pluginstrategy-registry)  
  - [2.4. Event-Driven & CQRS](#24-event-driven--cqrs)  
  - [2.5. Configuration-Driven Behavior](#25-configuration-driven-behavior)  
  - [2.6. Resilience & Observability](#26-resilience--observability)  
- [3. Module-Level Refactoring](#3-module-level-refactoring)  
  - [3.1. Lock Poisoning & Error Handling](#31-lock-poisoning--error-handling)  
  - [3.2. Replace TODO Stubs](#32-replace-todo-stubs)  
  - [3.3. Centralize Logging & Metrics](#33-centralize-logging--metrics)  
  - [3.4. Remove Sync/Async Duplication](#34-remove-syncasync-duplication)  
  - [3.5. Mmap & State Persistence](#35-mmap--state-persistence)  
  - [3.6. Feature-Flag Consistency](#36-feature-flag-consistency)  
  - [3.7. Documentation & Examples](#37-documentation--examples)  
- [4. Testing Strategy](#4-testing-strategy)  
  - [4.1. Integration-First Approach](#41-integration-first-approach)  
  - [4.2. Adapter Traits & Mocks](#42-adapter-traits--mocks)  
  - [4.3. End-to-End Scenarios](#43-end-to-end-scenarios)  
  - [4.4. Unit & Property Tests](#44-unit--property-tests)  
- [5. CI/CD & Release](#5-cicd--release)  
- [6. Next Steps](#6-next-steps)  

---

## 1. Goals

1. Enforce clear separation of concerns  
2. Improve testability (especially via integration-first)  
3. Centralize cross-cutting concerns (logging, metrics, config)  
4. Introduce resilience patterns (retries, circuit breakers)  
5. Streamline developer experience (CLI, examples, documentation)  

---

## 2. Architectural Refactoring

### 2.1. Clean/Hexagonal Layers  
- Create **domain/** crate for core entities & business rules (e.g., `MarketState`, strategies).  
- Create **application/** crate for use-cases, ports & orchestration.  
- Create **infrastructure/** crate for adapters: RPC, Kafka, file I/O, persistence.  
- Keep `main.rs` or a new `bootstrap.rs` as the thin composition root.

### 2.2. Dependency Injection & Composition  
- Define traits for outbound ports (e.g., `RpcAdapter`, `KafkaAdapter`, `StateRepository`).  
- Use a builder or small DI container in `bootstrap.rs` to wire real vs. test implementations.  
- Pass only trait objects to application logic.

### 2.3. Plugin/Strategy Registry  
- Abstract each trading strategy behind a `TradingStrategy` trait.  
- Register strategies by name/key, selecting via config or CLI flag.  
- Enable dynamic loading of new strategies without code changes.

### 2.4. Event-Driven & CQRS  
- Introduce an internal event bus (e.g., `tokio::sync::broadcast`) for domain events.  
- Split commands (mutations) from queries (reads) with explicit ports.  
- Allow multiple subscribers (metrics, persistence, side-effects) to react to events.

### 2.5. Configuration-Driven Behavior  
- Consolidate all settings into a single `AppConfig` (parsed via Serde from TOML/JSON/YAML).  
- Validate on startup, fail fast on errors.  
- Drive logging level, metrics toggles, strategy selection, retry policies entirely via config.

### 2.6. Resilience & Observability  
- Create middleware for retries (with back-off), circuit breakers, timeouts.  
- Integrate a common metrics facade; emit counters/gauges/histograms to Prometheus.  
- Add health-check HTTP endpoints or sidecar-compatible probes.

---

## 3. Module-Level Refactoring

### 3.1. Lock Poisoning & Error Handling  
- Replace `.read().unwrap()` / `.write().unwrap()` with `map_or_else` or `expect("lock poisoned")`.  
- Standardize errors via `thiserror`-backed enums instead of `Box<dyn Error>`.

### 3.2. Replace TODO Stubs  
- Implement real swap instruction builders (snipe/exit) or extract to pluggable modules.  
- Fail fast with clear error if no instructions can be generated.

### 3.3. Centralize Logging & Metrics  
- Move all `tracing_subscriber` setup into `shared/bitquery-solana-core`.  
- Provide a single `init_logging_and_metrics(config: &InitConfig)` entrypoint.

### 3.4. Remove Sync/Async Duplication  
- Split `network.rs` into `net-async` and `net-sync` modules/crates.  
- Expose only unified `NetworkClient` trait to application.

### 3.5. Mmap & State Persistence  
- Refactor `persist_to_mmap` to support dynamic growth (e.g., chunked files).  
- Abstract persistence behind a `StateRepository` trait for future DB support.

### 3.6. Feature-Flag Consistency  
- Standardize high-performance, metrics, and allocator flags at the workspace root.  
- Ensure all crates share the same defaults and naming.

### 3.7. Documentation & Examples  
- Add Rustdoc comments for all public types/functions.  
- Provide "getting started" examples for `ops-node` (config, strategies, shutdown).  

---

## 4. Testing Strategy

### 4.1. Integration-First Approach  
- Use Docker Compose to stand up Kafka & Solana Test Validator.  
- Write black-box tests using `assert_cmd` to launch the binary and assert on behavior.

### 4.2. Adapter Traits & Mocks  
- Define `RpcAdapter` & `KafkaAdapter` traits in `infrastructure/` crate.  
- Provide in-memory implementations for unit and integration tests.

### 4.3. End-to-End Scenarios  
- Test full snipe flow:  
  1. Produce "NewToken" event on Kafka  
  2. Consume "SnipeOrder" topic and verify instructions  
- Add failure scenarios (RPC errors, Kafka unavailability).

### 4.4. Unit & Property Tests  
- Expand unit tests for:  
  - state persistence  
  - retry strategies (back-off timings)  
  - network adapters  
  - strategy logic (edge cases)  

---

## 5. CI/CD & Release

- Add GitHub Actions workflows to:  
  - Build & test all crates in parallel  
  - Run integration tests behind services (with Docker Compose)  
  - Publish crates via `cargo publish` on tags  
  - Generate changelogs and release notes automatically  

---

## 6. Next Steps

1. Create `REFACTORING_PLAN.md` in repo root (this file).  
2. Scaffold new crates: `domain/`, `application/`, `infrastructure/`.  
3. Build adapter traits and move existing code behind them.  
4. Introduce config parsing and validation.  
5. Write first integration test and CI workflow.  
6. Incrementally migrate modules, ensuring tests pass at each step.  

---

> _This plan will evolve as we uncover deeper domain requirements and operational constraints._