# mini-lambda
A lightweight, Rust-powered serverless compute platform for running WebAssembly anywhere.

### IMPORTANT NOTE:
I plan on using this as the final project in one of my classes, so can't release code publicly due to academic honesty. Please reach out to me via [LinkedIn](https://www.linkedin.com/in/elliottfaa/) or [email](mailto:elliotthfaa@gmail.com) if you want more details about the project, I'd love to discuss!


# Project Overview (context for AI assistants and GitHub Copilot)

Here’s a detailed **project overview report** you can drop into VS Code for GitHub Copilot. It captures the vision, architecture, and development roadmap in one place so Copilot can give you better completions and context-aware help as you build.

---

# MiniLambda: Project Overview

## Vision

MiniLambda is a lightweight, Rust-powered serverless compute platform inspired by AWS Lambda. The goal is to let users submit WebAssembly (WASM) functions that can be executed securely across a mesh of personal devices, without relying on external cloud services. The system emphasizes **low cold start latency**, **secure sandboxing**, and **scalable orchestration**—while remaining simple enough to run locally.

## Core Goals

* **Serverless execution:** Allow users to upload WASM functions and run them anywhere, Lambda-style.
* **Low latency:** Achieve sub-50ms cold start latency via ahead-of-time (AOT) compilation and caching.
* **Distributed orchestration:** Dynamically discover and register worker nodes, track health, and distribute workloads efficiently.
* **Scalability:** Support horizontal scaling to 100+ nodes, with load-aware scheduling.
* **Security:** Provide sandboxed execution with capability restrictions (memory/timeouts, no network/FS by default).
* **Simplicity:** Use a pre-shared password/token for authentication. Avoid external cloud services, keeping all state in local SQLite and file-based storage.

## High-Level Architecture

### Components

1. **Client (CLI)**

   * Command-line tool for submitting jobs, checking status, streaming logs, and retrieving results.
   * Talks only to the orchestrator API (not directly to workers).
   * Provides ergonomic commands for packaging WASM modules + manifests + input payloads.

2. **Orchestrator (Control Plane)**

   * Manages job queue, worker registry, scheduling, and state transitions.
   * Exposes authenticated APIs (submit job, job status, logs, results, worker register/heartbeat, job leasing).
   * Implements load-aware scheduling and retries on failure.
   * Stores state in SQLite + a content-addressed blob store for modules, precompiled artifacts, and results.
   * Not in the MVP yet, but designed into the architecture.

3. **Worker/Node (Data Plane)**

   * Registers dynamically with the orchestrator and sends periodic heartbeats (capabilities, load).
   * Leases jobs from the orchestrator, executes WASM functions in a secure runtime, and streams logs back.
   * Enforces sandboxing (timeouts, memory limits, instruction fuel metering).
   * Can run heterogeneous hardware (e.g., laptop, server, Raspberry Pi).

### Shared Layer

* **Proto / Contracts crate:** Defines shared types (Job, Worker, Lease, Manifest, JobState), request/response models, error enums, and API route constants. Ensures orchestrator, workers, and client speak the same language.

## Code Structure (Cargo Workspace)

```
mini-lambda/
├─ Cargo.toml (workspace root)
├─ crates/
│  ├─ proto/      # lib crate: shared types, manifests, job states
│  ├─ client/     # bin crate: CLI for job submission and status
│  └─ worker/     # bin crate: node agent that executes jobs
```

* `proto` (lib): Job contracts, manifests, job state enums, request/response schemas.
* `client` (bin): CLI built with `clap` and `reqwest`, communicates with orchestrator API.
* `worker` (bin): Worker node runtime built with `tokio`, registers with orchestrator, executes WASM jobs using Wasmtime/Wasmer.

## Development Roadmap

### Phase 1 — Foundation

* [x] Set up Rust workspace with `proto`, `client`, and `worker` crates.
* [x] Define basic job manifest, job states, and request/response types.
* [x] Stub CLI and worker binaries.

### Phase 2 — Local Execution Prototype

* Implement WASM execution in the worker with timeouts, memory caps, and stdout capture.
* Add ahead-of-time compilation + caching to hit sub-50ms cold start targets.
* Add CLI command to run jobs against a local worker for quick iteration.

### Phase 3 — Orchestrator & APIs

* Build orchestrator with SQLite state and blob storage.
* Add worker registration, heartbeats, job queue, and leasing.
* Expose REST APIs for `submit`, `status`, `logs`, and `results`.

### Phase 4 — Distributed Execution

* Multiple workers dynamically register and accept leases.
* Orchestrator implements load-aware scheduling and retries.
* CLI supports streaming logs and fetching results.

### Phase 5 — Security & Hardening

* Replace plain password with pre-shared tokens (bearer/HMAC).
* Enforce WASM sandboxing with restricted capabilities (network/FS opt-in).
* Add TLS or Noise/mTLS for worker ↔ orchestrator communication.

### Phase 6 — Observability & UX

* Structured JSON logs for jobs, workers, and orchestrator state transitions.
* Metrics collection (jobs/sec, p95 latency, failure/retry counts).
* CLI improvements (watch mode, job table, worker status view).

---

## Stretch Goals

* Module registry: Reuse pre-uploaded modules without resubmission.
* Resource classes (“cpu-small”, “cpu-large”) and quotas.
* Dashboard for monitoring jobs and workers.
* Support for advanced WASM features (WASI-NN, GPU backends).

---

This document should give GitHub Copilot enough **context about the system architecture, goals, and roadmap** to provide meaningful completions and suggestions as you work.

---

Do you want me to also generate a **diagram (Mermaid or ASCII)** of the architecture (Client → Orchestrator → Workers), so you can drop it into the README right away?
