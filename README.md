# mini-lambda
A lightweight, Rust-powered serverless compute platform for running WebAssembly anywhere.

**Note**: This project is currently under active development and is incomplete, expect lots of changes. Feel free to reach out to me via [LinkedIn](https://www.linkedin.com/in/elliottfaa/) or [email](mailto:elliotthfaa@gmail.com) if you want more details about the project, I'd love to discuss!

### Features

- Serverless execution: Run uploaded WebAssembly (WASM) modules in a sandboxed environment.
- Low latency: Ahead-of-time compilation and caching to target sub-50ms cold start times.
- Distributed orchestration: Workers dynamically register and receive jobs from an orchestrator.
- Secure runtime: Sandbox isolation with configurable memory/time limits and restricted capabilities.
- Scalable design: Architecture designed to scale horizontally across 100+ heterogeneous nodes.
- Simple authentication: Pre-shared password/token system for communication between components.

### Components

- Client (CLI): Submit jobs, check status, fetch logs/results.
- Orchestrator: Manages job queue, worker registry, and scheduling policies.
- Worker/Node: Executes WASM modules securely, reporting results and heartbeats back to the orchestrator.
- Proto/Contracts: Shared types, manifests, and API request/response models.

### How to Run

**Coming soon!!**  
There are lots of changes being made still and API is not stable.

### Some Stretch Goals
- Web dashboard for monitoring jobs and workers
- More advanced authenticaion


### Todo
- Edit client to use orchestrator as intermediary