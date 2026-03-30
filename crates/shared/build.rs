fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
        .compile_protos(
            &["proto/shared.proto", "proto/client_api.proto", "proto/worker_api.proto", "proto/executor.proto"], 
            &["proto"]
        )?;

    Ok(())
}