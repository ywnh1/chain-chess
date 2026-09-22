━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  🪟 编译 Windows exe (x86_64-pc-windows-msvc)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

   Compiling serde_core v1.0.228
   Compiling windows-link v0.2.1
   Compiling winnow v1.0.3
   Compiling zerofrom v0.1.8
   Compiling stable_deref_trait v1.2.1
error[E0463]: can't find crate for `core`
  |
  = note: the `x86_64-pc-windows-msvc` target may not be installed
  = help: consider downloading the target with `rustup target add x86_64-pc-windows-msvc`

For more information about this error, try `rustc --explain E0463`.
error: could not compile `windows-link` (lib) due to 1 previous error
warning: build failed, waiting for other jobs to finish...
error: could not compile `stable_deref_trait` (lib) due to 1 previous error
error[E0463]: can't find crate for `std`
  |
  = note: the `x86_64-pc-windows-msvc` target may not be installed
  = help: consider downloading the target with `rustup target add x86_64-pc-windows-msvc`

error: could not compile `zerofrom` (lib) due to 1 previous error
error: could not compile `serde_core` (lib) due to 1 previous error
