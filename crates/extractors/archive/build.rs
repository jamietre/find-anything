// Probe for stabilisation of `std::alloc::set_alloc_error_hook`.
//
// This feature is currently nightly-only (rust-lang/rust#51245).  When it
// lands on stable it would let us convert OOM aborts into catchable panics,
// substantially improving 7z extraction on memory-constrained systems.
//
// We try to compile a tiny snippet that uses the function WITHOUT the
// `#![feature(alloc_error_hook)]` gate.  On current stable/nightly that
// fails (unstable items require the gate even on nightly).  When the feature
// is stabilised the probe succeeds, we set `alloc_error_hook_stable`, and a
// `compile_error!` in lib.rs fires to alert the developer.
fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let out_dir = std::env::var("OUT_DIR").unwrap();

    let probe_src = "fn main() { std::alloc::set_alloc_error_hook(|_| {}); }";
    let probe_path = std::path::Path::new(&out_dir).join("probe_alloc_error_hook.rs");

    if std::fs::write(&probe_path, probe_src).is_err() {
        return;
    }

    let stable = std::process::Command::new(&rustc)
        .args(["--edition", "2021", "--crate-type", "bin", "--emit", "metadata"])
        .arg("--out-dir").arg(&out_dir)
        .arg(&probe_path)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    let _ = std::fs::remove_file(&probe_path);

    // Declare the cfg key regardless so rustc doesn't warn about unknown cfgs.
    println!("cargo:rustc-check-cfg=cfg(alloc_error_hook_stable)");

    if stable {
        println!("cargo:rustc-cfg=alloc_error_hook_stable");
    }
}
