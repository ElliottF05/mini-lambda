use std::io::Read;

use wasmer::{Engine, Module};
use wasmer_wasix::{runners::wasi::{RuntimeOrEngine, WasiRunner}, Pipe};

use crate::{errors::WorkerError, job_ticket::JobTicket};


/// Runs the inputted wasm module in a separate tokio spawn_blocking thread
pub async fn run_wasm_module(engine: Engine, module: Module, run_args: Vec<String>, ticket: JobTicket) -> Result<String, WorkerError> {
    let run_result = tokio::task::spawn_blocking(move || -> Result<String, WorkerError> {

        let _ticket = ticket; // keep ticket alive for the duration of this function

        // create pipe pair for stdout
        let (stdout_sender, mut stdout_reader) = Pipe::channel();

        {
            // create and configure the runner
            let mut runner = WasiRunner::new();
            runner
                .with_stdout(Box::new(stdout_sender)) // use stdout to capture output
                .with_args(run_args); // send args

            // run the module (blocking)
            runner.run_wasm(
                RuntimeOrEngine::Engine(engine),
                "temp", // TODO: replace name
                module,
                wasmer_types::ModuleHash::random()
            ).map_err(|e| WorkerError::Execution(e.to_string()))?;
        }

        // read output and return it
        let mut buf = String::new();
        stdout_reader.read_to_string(&mut buf)?;
        Ok(buf)
    })
    .await?; // JoinHandle result

    return run_result;

}