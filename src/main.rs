use std::process;

use arm_assembler::{assemble, AsmConfig, Isa};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 3 {
        eprintln!(
            "Usage: {} <input.s> -o <output.bin> [--isa thumb|a32]",
            args[0]
        );
        process::exit(1);
    }

    let mut input = None;
    let mut output = None;
    let mut isa = None;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-o" => {
                i += 1;
                output = Some(args[i].clone());
            }
            "--isa" => {
                i += 1;
                isa = Some(match args[i].as_str() {
                    "thumb" => Isa::Thumb,
                    "a32" | "arm" => Isa::A32,
                    other => {
                        eprintln!("Unknown ISA: {other}");
                        process::exit(1);
                    }
                });
            }
            _ => {
                input = Some(args[i].clone());
            }
        }
        i += 1;
    }

    let input = input.unwrap_or_else(|| {
        eprintln!("No input file specified");
        process::exit(1);
    });
    let output = output.unwrap_or_else(|| {
        eprintln!("No output file specified (use -o)");
        process::exit(1);
    });

    let source = std::fs::read_to_string(&input).unwrap_or_else(|e| {
        eprintln!("Cannot read {input}: {e}");
        process::exit(1);
    });

    // Auto-detect ISA from source directives if not specified
    let default_isa = isa.unwrap_or_else(|| {
        if source.contains(".thumb") {
            Isa::Thumb
        } else if source.contains(".arm") {
            Isa::A32
        } else {
            Isa::A32
        }
    });

    let config = AsmConfig { default_isa };
    match assemble(&source, &config) {
        Ok(out) => {
            let bytes = out.text_bytes();
            std::fs::write(&output, bytes).unwrap_or_else(|e| {
                eprintln!("Cannot write {output}: {e}");
                process::exit(1);
            });
        }
        Err(e) => {
            eprintln!("{input}: {e}");
            process::exit(1);
        }
    }
}
