fn main() {
    let metadata: Vec<u8> = minijam_runtime::Runtime::metadata().into();
    let hex = metadata
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!("0x{hex}");
}
