//! Speech to text as a plug-in program (contract `qnc.transcribe` 2) with NVIDIA Parakeet TDT
//! through sherpa-onnx, on the processor. It is not autoregressive: a few seconds of speech take
//! a fraction of a second on two cores and it invents nothing on silence. The sherpa-onnx C
//! library is loaded from beside this program when the first job comes; without it the program
//! answers an error, nothing falls back. The model (a folder of encoder, decoder, joiner and
//! tokens) is loaded once and kept for the next job. Its priority is the launcher's.

mod engine;
mod sherpa;

fn main() {
    let root = match qnc_settings::root() {
        Ok(root) => root,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    qnc_diagnostics::init(&qnc_settings::diagnostics_dir(&root), "qnc-transcribe-parakeet");
    qnc_diagnostics::log_panics();
    let mut engine = engine::Parakeet::default();
    if let Err(error) = qnc_transcribe_jobs::serve(&root, &mut engine, "qnc.transcribe.parakeet", env!("CARGO_PKG_VERSION"), "sherpa-onnx parakeet-tdt") {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
