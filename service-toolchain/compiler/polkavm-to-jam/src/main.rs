// SPDX-License-Identifier: Apache-2.0
use std::{borrow::Cow, env, fs, path::PathBuf, process::ExitCode};

use jam_program_blob_common::ProgramBlob;
use polkavm_linker::{Config, ProgramParts, TargetInstructionSet};

fn run() -> Result<(), String> {
    let mut args = env::args_os().skip(1);
    let input = PathBuf::from(
        args.next()
            .ok_or("usage: minijam-polkavm-to-jam <input.elf> <output.blob> [output.polkavm]")?,
    );
    let output = PathBuf::from(
        args.next()
            .ok_or("usage: minijam-polkavm-to-jam <input.elf> <output.blob> [output.polkavm]")?,
    );
    let debug_output = args.next().map(PathBuf::from);
    if args.next().is_some() {
        return Err(
            "usage: minijam-polkavm-to-jam <input.elf> <output.blob> [output.polkavm]".into(),
        );
    }

    let elf = fs::read(&input).map_err(|error| format!("read {}: {error}", input.display()))?;
    let mut config = Config::default();
    config.set_strip(true);
    config.set_dispatch_table(vec![
        b"minijam_refine".to_vec(),
        b"minijam_accumulate".to_vec(),
    ]);
    let linked = polkavm_linker::program_from_elf(config, TargetInstructionSet::JamV1, &elf)
        .map_err(|error| format!("link {}: {error}", input.display()))?;

    if let Some(path) = debug_output {
        fs::write(&path, &linked).map_err(|error| format!("write {}: {error}", path.display()))?;
    }

    let parts = ProgramParts::from_bytes(linked.into())
        .map_err(|error| format!("decode linked PolkaVM program: {error}"))?;
    let blob = ProgramBlob::from_pvm(&parts, Cow::Borrowed(&[]))
        .to_vec()
        .map_err(str::to_owned)?;
    fs::write(&output, blob).map_err(|error| format!("write {}: {error}", output.display()))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
