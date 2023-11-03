/// NAME Mips Assembler
use crate::args::Args;
use crate::config::Config;
use crate::lineinfo::*;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::str;

fn mask_u8(n: u8, x: u8) -> Result<u8, &'static str> {
    let out = n & ((1 << x) - 1);
    if out != n {
        Err("Masking error")
    } else {
        Ok(out)
    }
}

fn mask_u32(n: u32, x: u8) -> Result<u32, &'static str> {
    let out = n & ((1 << x) - 1);
    if out != n {
        Err("Masking error")
    } else {
        Ok(out)
    }
}

fn base_parse(input: &str) -> Result<u32, &'static str> {
    if input.starts_with("0x") {
        // Hexadecimal
        u32::from_str_radix(&input[2..], 16).map_err(|_| "Failed to parse as hexadecimal")
    } else if input.starts_with("0b") {
        // Binary
        u32::from_str_radix(&input[2..], 2).map_err(|_| "Failed to parse as binary")
    } else if input.starts_with('0') && input.len() > 1 {
        // Octal
        u32::from_str_radix(&input[1..], 8).map_err(|_| "Failed to parse as octal")
    } else {
        // Decimal
        input
            .parse::<u32>()
            .map_err(|_| "Failed to parse as decimal")
    }
}

const TEXT_ADDRESS_BASE: u32 = 0x400000;
const DATA_ADDRESS_BASE: u32 = 0x10000000;
const MIPS_INSTR_BYTE_WIDTH: u32 = 4;

// Controls whether data is being assembled into .text, .data, etc
enum AssemblyMode {
    TextMode,
    DataMode,
}

/// The form of an R-type instruction, specificially
/// which arguments it expects in which order
enum RForm {
    RdRsRt,
    RdRtShamt,
}

/// The variable components of an R-type instruction
pub struct R {
    shamt: u8,
    funct: u8,
    form: RForm,
}

/// The form of an I-type instruction, specifically
/// which arguments it expects in which order
enum IForm {
    RtImm,
    RtImmRs,
    RtRsImm,
    RsRtLabel,
}

/// The variable components of an I-type instruction
pub struct I {
    opcode: u8,
    form: IForm,
}

/// The variable component of a J-type instruction
pub struct J {
    opcode: u8,
}

/// Parses an R-type instruction mnemonic into an [R]
pub fn r_operation(mnemonic: &str) -> Result<R, &'static str> {
    match mnemonic {
        "add" => Ok(R {
            shamt: 0,
            funct: 0x20,
            form: RForm::RdRsRt,
        }),
        "sub" => Ok(R {
            shamt: 0,
            funct: 0x22,
            form: RForm::RdRsRt,
        }),
        "sll" => Ok(R {
            shamt: 0,
            funct: 0x00,
            form: RForm::RdRtShamt,
        }),
        "srl" => Ok(R {
            shamt: 0,
            funct: 0x02,
            form: RForm::RdRtShamt,
        }),
        "xor" => Ok(R {
            shamt: 0,
            funct: 0x26,
            form: RForm::RdRsRt,
        }),
        _ => Err("Failed to match R-instr mnemonic"),
    }
}

/// Parses an I-type instruction mnemonic into an [I]
pub fn i_operation(mnemonic: &str) -> Result<I, &'static str> {
    match mnemonic {
        "ori" => Ok(I {
            opcode: 0xd,
            form: IForm::RtRsImm,
        }),
        "lb" => Ok(I {
            opcode: 0x20,
            form: IForm::RtImmRs,
        }),
        "lbu" => Ok(I {
            opcode: 0x24,
            form: IForm::RtImmRs,
        }),
        "lh" => Ok(I {
            opcode: 0x21,
            form: IForm::RtImmRs,
        }),
        "lhu" => Ok(I {
            opcode: 0x25,
            form: IForm::RtImmRs,
        }),
        "lw" => Ok(I {
            opcode: 0x23,
            form: IForm::RtImmRs,
        }),
        "ll" => Ok(I {
            opcode: 0x30,
            form: IForm::RtImmRs,
        }),
        "lui" => Ok(I {
            opcode: 0xf,
            form: IForm::RtImm,
        }),
        "sb" => Ok(I {
            opcode: 0x28,
            form: IForm::RtImmRs,
        }),
        "sh" => Ok(I {
            opcode: 0x29,
            form: IForm::RtImmRs,
        }),
        "sw" => Ok(I {
            opcode: 0x2b,
            form: IForm::RtImmRs,
        }),
        "sc" => Ok(I {
            opcode: 0x38,
            form: IForm::RtImmRs,
        }),
        "beq" => Ok(I {
            opcode: 0x4,
            form: IForm::RsRtLabel,
        }),
        "bne" => Ok(I {
            opcode: 0x5,
            form: IForm::RsRtLabel,
        }),
        _ => Err("Failed to match I-instr mnemonic"),
    }
}

/// Parses a J-type instruction mnemonic into a [J]
fn j_operation(mnemonic: &str) -> Result<J, &'static str> {
    match mnemonic {
        "j" => Ok(J { opcode: 0x2 }),
        "jal" => Ok(J { opcode: 0x3 }),
        _ => Err("Failed to match J-instr mnemonic"),
    }
}

/// Write a u32 into a file, zero-padded to 32 bits (4 bytes)
pub fn write_u32(mut file: &File, data: u32) -> std::io::Result<()> {
    fn convert_endianness(input: u32) -> u32 {
        ((input & 0x000000FF) << 24)
            | ((input & 0x0000FF00) << 8)
            | ((input & 0x00FF0000) >> 8)
            | ((input & 0xFF000000) >> 24)
    }

    const PADDED_LENGTH: usize = 4;

    // Create a 4-length buffer
    let mut padded_buffer: [u8; PADDED_LENGTH] = [0; PADDED_LENGTH];

    // Convert data into bytes
    let bytes: [u8; PADDED_LENGTH] = (convert_endianness(data)).to_be_bytes();

    // Copy bytes into buffer at offset s.t. value is left-padded with 0s
    let copy_index = PADDED_LENGTH - bytes.len();
    padded_buffer[copy_index..].copy_from_slice(&bytes);

    // Write to file
    file.write_all(&padded_buffer)
}

/// Converts a numbered mnemonic ($t0, $s8, etc) or literal (55, 67, etc) to its integer representation
fn reg_number(mnemonic: &str) -> Result<u8, &'static str> {
    if mnemonic.len() != 3 {
        return Err("Mnemonic out of bounds");
    }

    match mnemonic.chars().nth(2) {
        Some(c) => match c.to_digit(10) {
            Some(digit) => {
                if digit <= 31 {
                    Ok(digit as u8)
                } else {
                    Err("Expected u8")
                }
            }
            _ => Err("Invalid register index"),
        },
        _ => Err("Malformed mnemonic"),
    }
}

/// Given a register or number, assemble it into its integer representation
fn assemble_reg(mnemonic: &str) -> Result<u8, &'static str> {
    // match on everything after $
    match &mnemonic[1..] {
        "zero" => Ok(0),
        "at" => Ok(1),
        "gp" => Ok(28),
        "sp" => Ok(29),
        "fp" => Ok(30),
        "ra" => Ok(31),
        _ => {
            let n = reg_number(mnemonic)?;
            let reg = match mnemonic.chars().nth(1) {
                Some('v') => n + 2,
                Some('a') => n + 4,
                Some('t') => {
                    if n <= 7 {
                        n + 8
                    } else {
                        // t8, t9 = 24, 25
                        // 24 - 8 + n
                        n + 16
                    }
                }
                Some('s') => n + 16,
                _ => {
                    // Catch registers like $0
                    mnemonic.parse::<u8>().unwrap_or(99)
                }
            };
            if reg <= 31 {
                Ok(reg)
            } else {
                Err("Register out of bounds")
            }
        }
    }
}

/// Enforce a specific length for a given vector
fn enforce_length(arr: &Vec<&str>, len: usize) -> Result<u32, &'static str> {
    if arr.len() != len {
        Err("Failed length enforcement")
    } else {
        Ok(0)
    }
}

/// Assembles an R-type instruction
fn assemble_r(r_struct: R, r_args: Vec<&str>) -> Result<u32, &'static str> {
    let mut rs: u8;
    let mut rt: u8;
    let mut rd: u8;
    let mut shamt: u8;

    match r_struct.form {
        RForm::RdRsRt => {
            enforce_length(&r_args, 3)?;
            rd = assemble_reg(r_args[0])?;
            rs = assemble_reg(r_args[1])?;
            rt = assemble_reg(r_args[2])?;
            shamt = r_struct.shamt;
        }
        RForm::RdRtShamt => {
            enforce_length(&r_args, 3)?;
            rd = assemble_reg(r_args[0])?;
            rs = 0;
            rt = assemble_reg(r_args[1])?;
            shamt = match base_parse(r_args[2]) {
                Ok(v) => v as u8,
                Err(_) => return Err("Failed to parse shamt"),
            }
        }
    };

    let mut funct = r_struct.funct;

    // Mask
    rs = mask_u8(rs, 5)?;
    rt &= mask_u8(rt, 5)?;
    rd &= mask_u8(rd, 5)?;
    shamt &= mask_u8(shamt, 5)?;
    funct &= mask_u8(funct, 6)?;

    // opcode : 31 - 26
    let mut result = 0x000000;

    // rs :     25 - 21
    println!("rs: {}", rs);
    result = (result << 6) | u32::from(rs);

    // rt :     20 - 16
    println!("rt: {}", rt);
    result = (result << 5) | u32::from(rt);

    // rd :     15 - 11
    println!("rd: {}", rd);
    result = (result << 5) | u32::from(rd);

    // shamt : 10 - 6
    println!("shamt: {}", shamt);
    result = (result << 5) | u32::from(shamt);

    // funct : 5 - 0
    result = (result << 6) | u32::from(funct);

    println!(
        "0x{:0shortwidth$x} {:0width$b}",
        result,
        result,
        shortwidth = 8,
        width = 32
    );
    Ok(result)
}

/// Assembles an I-type instruction
fn assemble_i(
    i_struct: I,
    i_args: Vec<&str>,
    labels: &HashMap<&str, u32>,
    instr_address: u32,
) -> Result<u32, &'static str> {
    let mut rs: u8;
    let mut rt: u8;
    let imm: u16;

    match i_struct.form {
        IForm::RtImm => {
            enforce_length(&i_args, 2)?;
            rs = 0;
            rt = assemble_reg(i_args[0])?;
            imm = match base_parse(i_args[1]) {
                Ok(v) => v as u16,
                Err(_) => return Err("Failed to parse imm"),
            }
        }
        IForm::RtImmRs => {
            // Immediate can default to 0 if not included in instructions
            // such as 'ld $t0, ($t1)'
            let i_args_catch = if i_args.len() == 2 {
                let mut c = i_args.clone();
                c.insert(1, "0");
                c
            } else {
                i_args
            };
            enforce_length(&i_args_catch, 3)?;
            rt = assemble_reg(i_args_catch[0])?;
            imm = match base_parse(i_args_catch[1]) {
                Ok(v) => v as u16,
                Err(_) => return Err("Failed to parse imm"),
            };
            rs = assemble_reg(i_args_catch[2])?;
        }
        IForm::RsRtLabel => {
            enforce_length(&i_args, 3)?;
            rs = assemble_reg(i_args[0])?;
            rt = assemble_reg(i_args[1])?;
            match labels.get(i_args[2]) {
                // Subtract byte width due to branch delay
                Some(v) => imm = ((*v) - instr_address) as u16,
                None => return Err("Undeclared label"),
            }
        }
        IForm::RtRsImm => {
            enforce_length(&i_args, 3)?;
            rt = assemble_reg(i_args[0])?;
            rs = assemble_reg(i_args[1])?;
            imm = match base_parse(i_args[2]) {
                Ok(v) => v as u16,
                Err(_) => return Err("Failed to parse imm"),
            };
        }
    };

    let mut opcode = i_struct.opcode;

    // Mask
    println!("Masking rs");
    rs = mask_u8(rs, 5)?;
    println!("Masking rt");
    rt = mask_u8(rt, 5)?;
    println!("Masking opcode");
    opcode = mask_u8(opcode, 6)?;
    // No need to mask imm, it's already a u16

    // opcode : 31 - 26
    let mut result: u32 = opcode.into();

    // rs :     25 - 21
    println!("rs: {}", rs);
    result = (result << 5) | u32::from(rs);

    // rt :     20 - 16
    println!("rt: {}", rt);
    result = (result << 5) | u32::from(rt);

    // imm :    15 - 0
    println!("imm: {}", imm);
    result = (result << 16) | u32::from(imm);

    println!(
        "0x{:0shortwidth$x} {:0width$b}",
        result,
        result,
        shortwidth = 8,
        width = 32
    );
    Ok(result)
}

/// Assembles a J-type instruction
fn assemble_j(
    j_struct: J,
    j_args: Vec<&str>,
    labels: &HashMap<&str, u32>,
) -> Result<u32, &'static str> {
    enforce_length(&j_args, 1)?;

    let jump_address: u32 = labels[j_args[0]];
    println!("Masking jump address");
    println!("Jump address original: {}", jump_address);
    let mut masked_jump_address = mask_u32(jump_address, 28)?;
    println!("Jump address masked: {}", masked_jump_address);
    if jump_address != masked_jump_address {
        return Err("Tried to assemble illegal jump address");
    }

    // Byte-align jump address
    masked_jump_address >>= 2;

    let mut opcode = j_struct.opcode;

    // Mask
    println!("Masking opcode");
    opcode = mask_u8(opcode, 6)?;
    // No need to mask imm, it's already a u16

    // opcode : 31 - 26
    let mut result: u32 = opcode.into();

    // imm :    25 - 0
    println!("imm: {}", masked_jump_address);
    result = (result << 26) | masked_jump_address;

    println!(
        "0x{:0shortwidth$x} {:0width$b}",
        result,
        result,
        shortwidth = 8,
        width = 32
    );
    Ok(result)
}

use crate::parser::*;
use pest::Parser;

pub fn preprocess(input: String) -> String {
    let mut out = Vec::new();

    let mut eqv: HashMap<&str, &str> = HashMap::new();

    for mut line in input.lines() {
        line = line.trim();

        // Filter comments
        if let Some(pos) = line.find('#') {
            line = &line[0..pos]; // Remove the comment part
        }

        if line.is_empty() {
            continue;
        }

        let mut tokens: Vec<&str> = line.split_whitespace().collect();

        // Handle .eqv directives
        if tokens[0] == ".eqv" {
            eqv.insert(tokens[1], tokens[2]);
            continue;
        }

        // Replace via eqv
        tokens = tokens
            .iter()
            .map(|token| {
                if let Some(replacement) = eqv.get(*token) {
                    *replacement
                } else {
                    *token
                }
            })
            .collect();

        out.push(tokens.join(" "));
    }

    out.join("\n")
}

// General assembler entrypoint
pub fn assemble(program_config: &Config, program_arguments: &Args) -> Result<(), String> {
    // IO Setup
    let input_fn = &program_arguments.input_as;
    let output_fn = &program_arguments.output_as;

    let output_file: File = match File::create(output_fn) {
        Ok(v) => v,
        Err(_) => return Err("Failed to open output file".to_string()),
    };

    // Read input
    let mut file_contents: String = match fs::read_to_string(input_fn) {
        Ok(v) => v,
        Err(_) => return Err("Failed to read input file contents".to_string()),
    };

    // Preprocess
    file_contents = if program_config.preprocess {
        preprocess(file_contents)
    } else {
        file_contents
    };

    // Parse into CST
    let cst = parse_rule(
        MipsParser::parse(Rule::vernacular, file_contents.as_str())
            .expect("Failed to parse")
            .next()
            .unwrap(),
    );

    // Set up line info
    let lineinfo_fn = format!("{}.li", &program_arguments.output_as);
    let mut lineinfo: Vec<LineInfo> = vec![];

    let vernac_sequence: Vec<MipsCST> = if let MipsCST::Sequence(v) = cst {
        v
    } else {
        vec![cst]
    };

    // General setup
    let mut _assembly_mode = AssemblyMode::TextMode;
    let mut text_section_address: u32 = TEXT_ADDRESS_BASE;

    // Assign addresses to labels
    let mut labels: HashMap<&str, u32> = HashMap::new();
    for sub_cst in &vernac_sequence {
        match sub_cst {
            MipsCST::Label(label_str) => {
                println!(
                    "Inserting label {} at {:x}",
                    label_str, text_section_address
                );
                labels.insert(label_str, text_section_address);
                continue;
            }
            MipsCST::Directive(_, _) => continue,
            MipsCST::Sequence(_) => unreachable!(),
            _ => (),
        };

        text_section_address += MIPS_INSTR_BYTE_WIDTH
    }

    text_section_address = TEXT_ADDRESS_BASE;

    // Assemble instructions
    for sub_cst in vernac_sequence {
        match sub_cst {
            MipsCST::Instruction(mnemonic, args) => {
                // Update line info
                lineinfo.push(LineInfo {
                    instr_addr: text_section_address,
                    line_number: 0,
                    line_contents: instr_to_str(mnemonic, &args),
                    psuedo_op: "".to_string(),
                });

                if let Ok(instr_info) = r_operation(mnemonic) {
                    println!("-----------------------------------");
                    println!(
                        "[R] {} - shamt [{:x}] - funct [{:x}] - args [{:?}]",
                        mnemonic, instr_info.shamt, instr_info.funct, args
                    );
                    match assemble_r(instr_info, args) {
                        Ok(assembled_r) => {
                            if write_u32(&output_file, assembled_r).is_err() {
                                return Err("Failed to write to output binary".to_string());
                            }
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                } else if let Ok(instr_info) = i_operation(mnemonic) {
                    println!("-----------------------------------");
                    println!(
                        "[I] {} - opcode [{:x}] - args [{:?}]",
                        mnemonic, instr_info.opcode, args
                    );

                    match assemble_i(instr_info, args, &labels, text_section_address) {
                        Ok(assembled_i) => {
                            if write_u32(&output_file, assembled_i).is_err() {
                                return Err("Failed to write to output binary".to_string());
                            }
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                } else if let Ok(instr_info) = j_operation(mnemonic) {
                    println!("-----------------------------------");
                    println!(
                        "[J] {} - opcode [{:x}] - args [{:?}]",
                        mnemonic, instr_info.opcode, args
                    );

                    match assemble_j(instr_info, args, &labels) {
                        Ok(assembled_j) => {
                            if write_u32(&output_file, assembled_j).is_err() {
                                return Err("Failed to write to output binary".to_string());
                            }
                        }
                        Err(e) => return Err(e.to_string()),
                    }
                } else {
                    return Err("Failed to match instruction".to_string());
                }
            }
            MipsCST::Directive(mnemonic, args) => match mnemonic {
                "text" => _assembly_mode = AssemblyMode::TextMode,
                "data" => _assembly_mode = AssemblyMode::DataMode,
                "eqv" => (),
                _ => return Err(format!("Directive .{} not yet supported", mnemonic).to_string()),
            },
            _ => continue,
        };

        text_section_address += MIPS_INSTR_BYTE_WIDTH;
    }

    if program_arguments.line_info {
        if let Err(e) = lineinfo_export(lineinfo_fn, lineinfo) {
            return Err(e.to_string());
        }
    }

    Ok(())
}
