//! Integration tests comparing our assembler output against GNU arm-none-eabi-as.

use arm_assembler::{assemble, AsmConfig, Cpu, Isa};
use std::io::Write;
use std::process::Command;

/// Assemble with GNU as and extract raw .text bytes.
fn gnu_assemble(source: &str, cpu: Cpu) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("tempdir");
    let asm_path = dir.path().join("input.s");
    let obj_path = dir.path().join("output.o");
    let bin_path = dir.path().join("output.bin");

    std::fs::File::create(&asm_path)
        .unwrap()
        .write_all(source.as_bytes())
        .unwrap();

    let status = Command::new("arm-none-eabi-as")
        .args([
            &format!("-mcpu={}", cpu.gnu_name()),
            "-o",
            obj_path.to_str().unwrap(),
            asm_path.to_str().unwrap(),
        ])
        .status()
        .expect("failed to run arm-none-eabi-as");
    assert!(status.success(), "arm-none-eabi-as failed");

    let status = Command::new("arm-none-eabi-objcopy")
        .args([
            "-O",
            "binary",
            "-j",
            ".text",
            obj_path.to_str().unwrap(),
            bin_path.to_str().unwrap(),
        ])
        .status()
        .expect("failed to run arm-none-eabi-objcopy");
    assert!(status.success(), "arm-none-eabi-objcopy failed");

    std::fs::read(&bin_path).unwrap_or_default()
}

fn check_thumb(source: &str, cpu: Cpu) {
    let full_source = format!(".syntax unified\n.thumb\n{source}\n");
    let expected = gnu_assemble(&full_source, cpu);
    let output = assemble(
        &full_source,
        &AsmConfig {
            default_isa: Isa::Thumb,
        },
    )
    .unwrap();
    let actual = output.text_bytes();
    assert_eq!(
        actual, &expected[..],
        "mismatch for Thumb:\n  source: {source}\n  expected: {expected:02x?}\n  actual:   {actual:02x?}"
    );
}

fn check_a32(source: &str, cpu: Cpu) {
    let full_source = format!(".syntax unified\n.arm\n{source}\n");
    let expected = gnu_assemble(&full_source, cpu);
    let output = assemble(
        &full_source,
        &AsmConfig {
            default_isa: Isa::A32,
        },
    )
    .unwrap();
    let actual = output.text_bytes();
    assert_eq!(
        actual, &expected[..],
        "mismatch for A32:\n  source: {source}\n  expected: {expected:02x?}\n  actual:   {actual:02x?}"
    );
}

// ---------------------------------------------------------------------------
// Thumb tests
// ---------------------------------------------------------------------------

#[test]
fn thumb_movs_imm() {
    check_thumb("movs r0, #0", Cpu::CortexM4);
    check_thumb("movs r0, #42", Cpu::CortexM4);
    check_thumb("movs r7, #255", Cpu::CortexM4);
}

#[test]
fn thumb_mov_reg() {
    check_thumb("mov r0, r1", Cpu::CortexM4);
    check_thumb("mov r8, r3", Cpu::CortexM4);
    check_thumb("mov r0, r12", Cpu::CortexM4);
}

#[test]
fn thumb_adds_imm() {
    check_thumb("adds r0, r1, #3", Cpu::CortexM4);
    check_thumb("adds r2, #100", Cpu::CortexM4);
}

#[test]
fn thumb_adds_reg() {
    check_thumb("adds r0, r1, r2", Cpu::CortexM4);
}

#[test]
fn thumb_subs() {
    check_thumb("subs r0, r1, #3", Cpu::CortexM4);
    check_thumb("subs r3, #50", Cpu::CortexM4);
    check_thumb("subs r0, r1, r2", Cpu::CortexM4);
}

#[test]
fn thumb_cmp() {
    check_thumb("cmp r0, #10", Cpu::CortexM4);
    check_thumb("cmp r3, r5", Cpu::CortexM4);
}

#[test]
fn thumb_alu() {
    check_thumb("ands r0, r1", Cpu::CortexM4);
    check_thumb("eors r0, r1", Cpu::CortexM4);
    check_thumb("orrs r0, r1", Cpu::CortexM4);
    check_thumb("bics r2, r3", Cpu::CortexM4);
    check_thumb("mvns r4, r5", Cpu::CortexM4);
    check_thumb("tst r0, r1", Cpu::CortexM4);
    check_thumb("muls r0, r1, r0", Cpu::CortexM4);
}

#[test]
fn thumb_shifts() {
    check_thumb("lsls r0, r1, #3", Cpu::CortexM4);
    check_thumb("lsrs r2, r3, #8", Cpu::CortexM4);
    check_thumb("asrs r4, r5, #1", Cpu::CortexM4);
    check_thumb("lsls r0, r1", Cpu::CortexM4);
}

#[test]
fn thumb_ldr_str_imm() {
    check_thumb("ldr r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldr r0, [r1, #4]", Cpu::CortexM4);
    check_thumb("ldr r0, [r1, #124]", Cpu::CortexM4);
    check_thumb("str r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("str r2, [r3, #8]", Cpu::CortexM4);
}

#[test]
fn thumb_ldr_str_sp() {
    check_thumb("ldr r0, [sp, #0]", Cpu::CortexM4);
    check_thumb("ldr r0, [sp, #1020]", Cpu::CortexM4);
    check_thumb("str r0, [sp, #4]", Cpu::CortexM4);
}

#[test]
fn thumb_ldrb_strb() {
    check_thumb("ldrb r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldrb r0, [r1, #31]", Cpu::CortexM4);
    check_thumb("strb r2, [r3, #5]", Cpu::CortexM4);
}

#[test]
fn thumb_ldrh_strh() {
    check_thumb("ldrh r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldrh r0, [r1, #62]", Cpu::CortexM4);
    check_thumb("strh r2, [r3, #4]", Cpu::CortexM4);
}

#[test]
fn thumb_push_pop() {
    check_thumb("push {r0, r1, r2, lr}", Cpu::CortexM4);
    check_thumb("push {r4, r5, r6, r7, lr}", Cpu::CortexM4);
    check_thumb("pop {r0, r1, r2, pc}", Cpu::CortexM4);
    check_thumb("pop {r4, r5, r6, r7, pc}", Cpu::CortexM4);
}

#[test]
fn thumb_branch_cond() {
    check_thumb("beq target\nnop\ntarget:\nnop", Cpu::CortexM4);
    check_thumb("bne target\nnop\ntarget:\nnop", Cpu::CortexM4);
}

#[test]
fn thumb_branch_uncond() {
    check_thumb("b target\nnop\ntarget:\nnop", Cpu::CortexM4);
    // Backward branch
    check_thumb("target:\nnop\nb target", Cpu::CortexM4);
    // Backward branch over multiple instructions
    check_thumb("loop:\nnop\nnop\nnop\nb loop", Cpu::CortexM4);
    // Conditional backward branch (typical loop)
    check_thumb("loop:\nsubs r0, #1\nbne loop", Cpu::CortexM4);
}

#[test]
fn thumb_bl() {
    check_thumb("bl target\nnop\ntarget:\nnop", Cpu::CortexM4);
    // Backward bl
    check_thumb("target:\nnop\nbl target", Cpu::CortexM4);
}

#[test]
fn thumb_bx() {
    check_thumb("bx lr", Cpu::CortexM4);
    check_thumb("bx r0", Cpu::CortexM4);
}

#[test]
fn thumb_nop() {
    check_thumb("nop", Cpu::CortexM4);
}

#[test]
fn thumb_svc() {
    check_thumb("svc #0", Cpu::CortexM4);
    check_thumb("svc #255", Cpu::CortexM4);
}

#[test]
fn thumb_sub_sp() {
    check_thumb("sub sp, sp, #8", Cpu::CortexM4);
    check_thumb("sub sp, sp, #128", Cpu::CortexM4);
    check_thumb("add sp, sp, #16", Cpu::CortexM4);
}

#[test]
fn thumb_misc() {
    check_thumb("rev r0, r1", Cpu::CortexM4);
    check_thumb("rev16 r2, r3", Cpu::CortexM4);
    check_thumb("uxtb r0, r1", Cpu::CortexM4);
    check_thumb("uxth r0, r1", Cpu::CortexM4);
    check_thumb("sxtb r0, r1", Cpu::CortexM4);
    check_thumb("sxth r0, r1", Cpu::CortexM4);
}

#[test]
fn thumb_add_sp_rd() {
    check_thumb("add r0, sp, #0", Cpu::CortexM4);
    check_thumb("add r3, sp, #1020", Cpu::CortexM4);
}

// ---------------------------------------------------------------------------
// A32 tests
// ---------------------------------------------------------------------------

#[test]
fn a32_mov_imm() {
    check_a32("mov r5, #0", Cpu::CortexA7);
    check_a32("mov r3, #42", Cpu::CortexA7);
    check_a32("mov r10, #0xFF", Cpu::CortexA7);
}

#[test]
fn a32_mov_reg() {
    check_a32("mov r5, r8", Cpu::CortexA7);
    check_a32("mov r3, r12", Cpu::CortexA7);
}

#[test]
fn a32_add_sub() {
    check_a32("add r4, r7, #1", Cpu::CortexA7);
    check_a32("add r5, r8, r10", Cpu::CortexA7);
    check_a32("sub r3, r9, #10", Cpu::CortexA7);
    check_a32("subs r6, r4, r7", Cpu::CortexA7);
}

#[test]
fn a32_cmp() {
    check_a32("cmp r5, #10", Cpu::CortexA7);
    check_a32("cmp r8, r10", Cpu::CortexA7);
}

#[test]
fn a32_logic() {
    check_a32("and r4, r7, #0xFF", Cpu::CortexA7);
    check_a32("orr r5, r8, r10", Cpu::CortexA7);
    check_a32("eor r3, r9, #1", Cpu::CortexA7);
    check_a32("bic r6, r4, r7", Cpu::CortexA7);
    check_a32("tst r5, #0xFF", Cpu::CortexA7);
    check_a32("teq r8, r10", Cpu::CortexA7);
    check_a32("mvn r3, r9", Cpu::CortexA7);
    check_a32("rsb r4, r7, #0", Cpu::CortexA7);
}

#[test]
fn a32_ldr_str() {
    check_a32("ldr r5, [r8]", Cpu::CortexA7);
    check_a32("ldr r3, [r9, #4]", Cpu::CortexA7);
    check_a32("ldr r7, [r4, #-4]", Cpu::CortexA7);
    check_a32("str r10, [r6]", Cpu::CortexA7);
    check_a32("str r4, [r9, #8]", Cpu::CortexA7);
    check_a32("ldrb r5, [r8, #0]", Cpu::CortexA7);
    check_a32("strb r3, [r7, #10]", Cpu::CortexA7);
}

#[test]
fn a32_ldr_str_reg() {
    check_a32("ldr r5, [r8, r3]", Cpu::CortexA7);
    check_a32("str r4, [r9, r7]", Cpu::CortexA7);
    check_a32("ldr r3, [r6, r10, lsl #2]", Cpu::CortexA7);
}

#[test]
fn a32_ldrh_strh() {
    check_a32("ldrh r5, [r8]", Cpu::CortexA7);
    check_a32("ldrh r3, [r9, #10]", Cpu::CortexA7);
    check_a32("strh r7, [r4]", Cpu::CortexA7);
    check_a32("strh r10, [r6, #20]", Cpu::CortexA7);
    check_a32("ldrsh r5, [r8]", Cpu::CortexA7);
    check_a32("ldrsb r3, [r9, #5]", Cpu::CortexA7);
}

#[test]
fn a32_ldrd_strd() {
    check_a32("ldrd r4, r5, [r8]", Cpu::CortexA7);
    check_a32("ldrd r2, r3, [r9, #8]", Cpu::CortexA7);
    check_a32("strd r6, r7, [r4]", Cpu::CortexA7);
    check_a32("strd r8, r9, [r3, #16]", Cpu::CortexA7);
}

#[test]
fn a32_ldm_stm() {
    check_a32("ldm r5!, {r0, r1, r2}", Cpu::CortexA7);
    check_a32("stm r4!, {r0, r1, r2}", Cpu::CortexA7);
    check_a32("ldmdb r8!, {r3, r4, r5}", Cpu::CortexA7);
    check_a32("stmdb r9!, {r3, r4, r5}", Cpu::CortexA7);
}

#[test]
fn a32_branch() {
    // Forward branches
    check_a32("b target\nnop\ntarget:\nnop", Cpu::CortexA7);
    check_a32("bl target\nnop\ntarget:\nnop", Cpu::CortexA7);
    check_a32("beq target\nnop\ntarget:\nnop", Cpu::CortexA7);
    // Backward branches
    check_a32("target:\nnop\nb target", Cpu::CortexA7);
    check_a32("target:\nnop\nbl target", Cpu::CortexA7);
    check_a32("target:\nnop\nbeq target", Cpu::CortexA7);
    // Backward branch over multiple instructions
    check_a32("loop:\nnop\nnop\nnop\nb loop", Cpu::CortexA7);
    // Conditional backward branch (typical loop)
    check_a32("loop:\nsubs r0, r0, #1\nbne loop", Cpu::CortexA7);
}

#[test]
fn a32_bx_blx() {
    check_a32("bx lr", Cpu::CortexA7);
    check_a32("bx r5", Cpu::CortexA7);
    check_a32("blx r8", Cpu::CortexA7);
}

#[test]
fn a32_push_pop() {
    check_a32("push {r4, r5, lr}", Cpu::CortexA7);
    check_a32("pop {r4, r5, pc}", Cpu::CortexA7);
}

#[test]
fn a32_mul_family() {
    check_a32("mul r4, r7, r9", Cpu::CortexA7);
    check_a32("mla r5, r8, r3, r10", Cpu::CortexA7);
    check_a32("mls r6, r9, r4, r11", Cpu::CortexA7);
    check_a32("smull r2, r4, r6, r8", Cpu::CortexA7);
    check_a32("umull r3, r5, r7, r9", Cpu::CortexA7);
    check_a32("smlal r4, r6, r8, r10", Cpu::CortexA7);
    check_a32("umlal r5, r7, r9, r11", Cpu::CortexA7);
}

#[test]
fn a32_div() {
    check_a32("sdiv r4, r7, r9", Cpu::CortexA7);
    check_a32("udiv r5, r8, r3", Cpu::CortexA7);
}

#[test]
fn a32_movw_movt() {
    check_a32("movw r5, #1234", Cpu::CortexA7);
    check_a32("movw r10, #0xFFFF", Cpu::CortexA7);
    check_a32("movt r3, #0x1234", Cpu::CortexA7);
    check_a32("movt r8, #0", Cpu::CortexA7);
}

#[test]
fn a32_nop() {
    check_a32("nop", Cpu::CortexA7);
}

#[test]
fn a32_svc() {
    check_a32("svc #0", Cpu::CortexA7);
    check_a32("svc #42", Cpu::CortexA7);
}

#[test]
fn a32_shifts() {
    check_a32("lsl r5, r8, #3", Cpu::CortexA7);
    check_a32("lsr r3, r9, #8", Cpu::CortexA7);
    check_a32("asr r7, r4, #1", Cpu::CortexA7);
    check_a32("ror r10, r6, #12", Cpu::CortexA7);
    check_a32("lsl r5, r8, r3", Cpu::CortexA7);
    check_a32("lsr r4, r9, r7", Cpu::CortexA7);
}

#[test]
fn a32_shifted_operand() {
    check_a32("add r5, r8, r3, lsl #3", Cpu::CortexA7);
    check_a32("mov r4, r9, asr #5", Cpu::CortexA7);
    check_a32("sub r7, r4, r10, lsr #2", Cpu::CortexA7);
}

#[test]
fn a32_clz_rbit() {
    check_a32("clz r5, r8", Cpu::CortexA7);
    check_a32("clz r3, r10", Cpu::CortexA7);
    check_a32("rbit r7, r4", Cpu::CortexA7);
    check_a32("rbit r9, r6", Cpu::CortexA7);
}

#[test]
fn a32_bfi_bfc_bfx() {
    check_a32("bfi r5, r8, #4, #8", Cpu::CortexA7);
    check_a32("bfi r10, r3, #0, #16", Cpu::CortexA7);
    check_a32("bfc r7, #0, #16", Cpu::CortexA7);
    check_a32("ubfx r4, r9, #4, #8", Cpu::CortexA7);
    check_a32("sbfx r6, r10, #4, #8", Cpu::CortexA7);
}

#[test]
fn a32_extend() {
    check_a32("sxth r5, r8", Cpu::CortexA7);
    check_a32("sxtb r3, r10", Cpu::CortexA7);
    check_a32("uxth r7, r4", Cpu::CortexA7);
    check_a32("uxtb r9, r6", Cpu::CortexA7);
}

#[test]
fn a32_extend_add() {
    check_a32("sxtah r4, r7, r9", Cpu::CortexA7);
    check_a32("sxtab r5, r8, r10", Cpu::CortexA7);
    check_a32("uxtah r3, r6, r11", Cpu::CortexA7);
    check_a32("uxtab r8, r4, r7", Cpu::CortexA7);
}

#[test]
fn a32_rev() {
    check_a32("rev r5, r8", Cpu::CortexA7);
    check_a32("rev16 r3, r10", Cpu::CortexA7);
    check_a32("revsh r7, r4", Cpu::CortexA7);
}

#[test]
fn a32_exclusive() {
    check_a32("ldrex r5, [r8]", Cpu::CortexA7);
    check_a32("strex r4, r7, [r10]", Cpu::CortexA7);
    check_a32("ldrexb r5, [r9]", Cpu::CortexA7);
    check_a32("strexb r4, r7, [r10]", Cpu::CortexA7);
    check_a32("ldrexh r3, [r8]", Cpu::CortexA7);
    check_a32("strexh r6, r9, [r11]", Cpu::CortexA7);
    check_a32("clrex", Cpu::CortexA7);
}

#[test]
fn a32_mrs_msr() {
    check_a32("mrs r5, APSR", Cpu::CortexA7);
    check_a32("msr APSR_nzcvq, r7", Cpu::CortexA7);
}

#[test]
fn a32_ssat_usat() {
    check_a32("ssat r5, #16, r8", Cpu::CortexA7);
    check_a32("usat r7, #16, r4", Cpu::CortexA7);
}

#[test]
fn a32_sat_arith() {
    check_a32("qadd r4, r7, r9", Cpu::CortexA7);
    check_a32("qdadd r5, r8, r3", Cpu::CortexA7);
    check_a32("qsub r6, r10, r4", Cpu::CortexA7);
    check_a32("qdsub r3, r9, r7", Cpu::CortexA7);
}

#[test]
fn a32_packing() {
    check_a32("pkhbt r4, r7, r9", Cpu::CortexA7);
    check_a32("pkhbt r5, r8, r3, lsl #4", Cpu::CortexA7);
    check_a32("sel r6, r10, r4", Cpu::CortexA7);
}

#[test]
fn a32_dsp_mul() {
    check_a32("smulbb r4, r7, r9", Cpu::CortexA7);
    check_a32("smulbt r5, r8, r3", Cpu::CortexA7);
    check_a32("smultb r6, r10, r4", Cpu::CortexA7);
    check_a32("smultt r3, r9, r7", Cpu::CortexA7);
    check_a32("smmul r8, r5, r10", Cpu::CortexA7);
    check_a32("smuad r4, r6, r3", Cpu::CortexA7);
    check_a32("smusd r7, r9, r5", Cpu::CortexA7);
}

#[test]
fn a32_dsp_mul_acc() {
    check_a32("smlabb r4, r7, r9, r3", Cpu::CortexA7);
    check_a32("smmla r5, r8, r3, r10", Cpu::CortexA7);
    check_a32("smmls r6, r9, r4, r11", Cpu::CortexA7);
    check_a32("smlad r3, r10, r5, r7", Cpu::CortexA7);
    check_a32("smlsd r8, r4, r6, r9", Cpu::CortexA7);
    check_a32("usad8 r5, r7, r10", Cpu::CortexA7);
    check_a32("usada8 r4, r8, r3, r6", Cpu::CortexA7);
}

#[test]
fn a32_dsp_long_mul() {
    check_a32("smlalbb r4, r6, r8, r10", Cpu::CortexA7);
    check_a32("smlalbt r3, r5, r7, r9", Cpu::CortexA7);
    check_a32("smlaltb r2, r8, r4, r10", Cpu::CortexA7);
    check_a32("smlaltt r5, r7, r9, r11", Cpu::CortexA7);
    check_a32("smlald r4, r6, r8, r3", Cpu::CortexA7);
    check_a32("smlsld r3, r5, r7, r10", Cpu::CortexA7);
}

#[test]
fn a32_parallel_arith() {
    check_a32("sadd16 r4, r7, r9", Cpu::CortexA7);
    check_a32("sadd8 r5, r8, r3", Cpu::CortexA7);
    check_a32("uadd16 r8, r5, r10", Cpu::CortexA7);
    check_a32("uadd8 r4, r6, r3", Cpu::CortexA7);
    check_a32("qadd16 r5, r7, r10", Cpu::CortexA7);
    check_a32("sasx r4, r8, r5", Cpu::CortexA7);
    check_a32("usax r3, r5, r8", Cpu::CortexA7);
}

#[test]
fn a32_barriers() {
    check_a32("dmb sy", Cpu::CortexA7);
    check_a32("dsb sy", Cpu::CortexA7);
    check_a32("isb sy", Cpu::CortexA7);
}

#[test]
fn a32_hints() {
    check_a32("wfi", Cpu::CortexA7);
    check_a32("wfe", Cpu::CortexA7);
    check_a32("sev", Cpu::CortexA7);
}

#[test]
fn a32_bkpt() {
    check_a32("bkpt #0", Cpu::CortexA7);
    check_a32("bkpt #42", Cpu::CortexA7);
}

// ---------------------------------------------------------------------------
// Multi-instruction / realistic sequences
// ---------------------------------------------------------------------------

#[test]
fn thumb_fibonacci_like() {
    let src = "\
        movs r0, #0
        movs r1, #1
        movs r2, #10
    loop:
        adds r3, r0, r1
        mov r0, r1
        mov r1, r3
        subs r2, #1
        bne loop
    ";
    check_thumb(src, Cpu::CortexM4);
}

#[test]
fn a32_function_prologue() {
    let src = "\
        push {r4, r5, r6, r7, lr}
        sub sp, sp, #16
        mov r4, r0
        mov r5, r1
    ";
    check_a32(src, Cpu::CortexR5);
}

// ---------------------------------------------------------------------------
// Thumb-2 (32-bit) tests
// ---------------------------------------------------------------------------

#[test]
fn thumb2_movw_movt() {
    check_thumb("movw r0, #0", Cpu::CortexM4);
    check_thumb("movw r5, #1234", Cpu::CortexM4);
    check_thumb("movw r10, #0xFFFF", Cpu::CortexM4);
    check_thumb("movt r3, #0", Cpu::CortexM4);
    check_thumb("movt r9, #0x1234", Cpu::CortexM4);
}

#[test]
fn thumb2_wide_data_proc_imm() {
    check_thumb("add.w r3, r7, #0", Cpu::CortexM4);
    check_thumb("add.w r8, r8, #255", Cpu::CortexM4);
    check_thumb("sub.w r5, r9, #100", Cpu::CortexM4);
    check_thumb("and.w r4, r6, #0xFF00FF", Cpu::CortexM4);
    check_thumb("orr.w r10, r3, #0xFF00FF00", Cpu::CortexM4);
    check_thumb("eor.w r2, r11, #0x10001", Cpu::CortexM4);
    check_thumb("bic.w r7, r4, #0xFF", Cpu::CortexM4);
    check_thumb("orn r6, r8, #0xFF", Cpu::CortexM4);
}

#[test]
fn thumb2_wide_data_proc_reg() {
    check_thumb("add.w r3, r5, r7", Cpu::CortexM4);
    check_thumb("sub.w r8, r9, r10", Cpu::CortexM4);
    check_thumb("and.w r4, r6, r11", Cpu::CortexM4);
    check_thumb("orr.w r2, r7, r3", Cpu::CortexM4);
    check_thumb("eor.w r5, r10, r1", Cpu::CortexM4);
    check_thumb("add.w r6, r8, r3, lsl #3", Cpu::CortexM4);
    check_thumb("sub.w r9, r4, r7, lsr #4", Cpu::CortexM4);
}

#[test]
fn thumb2_cmp_wide() {
    check_thumb("cmp.w r5, #256", Cpu::CortexM4);
    check_thumb("cmp.w r8, r10", Cpu::CortexM4);
}

#[test]
fn thumb2_shifts_wide() {
    check_thumb("lsl.w r3, r7, #5", Cpu::CortexM4);
    check_thumb("lsr.w r8, r4, #16", Cpu::CortexM4);
    check_thumb("asr.w r5, r9, #8", Cpu::CortexM4);
    check_thumb("ror.w r10, r2, #12", Cpu::CortexM4);
    check_thumb("lsl.w r6, r3, r8", Cpu::CortexM4);
    check_thumb("lsr.w r4, r9, r7", Cpu::CortexM4);
}

#[test]
fn thumb2_mul_div() {
    check_thumb("mul r4, r7, r9", Cpu::CortexM4);
    check_thumb("mla r5, r8, r3, r10", Cpu::CortexM4);
    check_thumb("mls r6, r9, r4, r11", Cpu::CortexM4);
    check_thumb("sdiv r3, r7, r10", Cpu::CortexM4);
    check_thumb("udiv r8, r4, r6", Cpu::CortexM4);
    check_thumb("smull r2, r4, r6, r8", Cpu::CortexM4);
    check_thumb("umull r3, r5, r7, r9", Cpu::CortexM4);
    check_thumb("smlal r4, r6, r8, r10", Cpu::CortexM4);
    check_thumb("umlal r5, r7, r9, r11", Cpu::CortexM4);
}

#[test]
fn thumb2_ldr_str_wide() {
    check_thumb("ldr.w r5, [r8]", Cpu::CortexM4);
    check_thumb("ldr.w r3, [r9, #100]", Cpu::CortexM4);
    check_thumb("str.w r7, [r4]", Cpu::CortexM4);
    check_thumb("str.w r10, [r6, #100]", Cpu::CortexM4);
    check_thumb("ldr.w r8, [r3, r5]", Cpu::CortexM4);
    check_thumb("str.w r4, [r9, r7]", Cpu::CortexM4);
}

#[test]
fn thumb2_clz_rbit() {
    check_thumb("clz r3, r7", Cpu::CortexM4);
    check_thumb("clz r8, r10", Cpu::CortexM4);
    check_thumb("rbit r5, r9", Cpu::CortexM4);
    check_thumb("rbit r4, r11", Cpu::CortexM4);
}

#[test]
fn thumb2_bfi_bfc_bfx() {
    check_thumb("bfi r5, r8, #4, #8", Cpu::CortexM4);
    check_thumb("bfi r10, r3, #0, #16", Cpu::CortexM4);
    check_thumb("bfc r7, #0, #16", Cpu::CortexM4);
    check_thumb("ubfx r4, r9, #4, #8", Cpu::CortexM4);
    check_thumb("ubfx r8, r3, #0, #12", Cpu::CortexM4);
    check_thumb("sbfx r6, r10, #4, #8", Cpu::CortexM4);
}

#[test]
fn thumb2_exclusive() {
    check_thumb("ldrex r5, [r8]", Cpu::CortexM4);
    check_thumb("ldrex r3, [r9, #4]", Cpu::CortexM4);
    check_thumb("strex r4, r7, [r10]", Cpu::CortexM4);
    check_thumb("strex r3, r8, [r6]", Cpu::CortexM4);
    check_thumb("ldrexb r5, [r9]", Cpu::CortexM4);
    check_thumb("strexb r4, r7, [r10]", Cpu::CortexM4);
    check_thumb("ldrexh r3, [r8]", Cpu::CortexM4);
    check_thumb("strexh r6, r9, [r11]", Cpu::CortexM4);
}

#[test]
fn thumb2_cbz_cbnz() {
    check_thumb("cbz r3, target\nnop\ntarget:", Cpu::CortexM4);
    check_thumb("cbz r7, target\nnop\ntarget:", Cpu::CortexM4);
    check_thumb("cbnz r5, target\nnop\ntarget:", Cpu::CortexM4);
}

#[test]
fn thumb2_it_block() {
    check_thumb("it eq\nmoveq r0, r1", Cpu::CortexM4);
    check_thumb("ite ne\nmovne r3, r5\nmoveq r3, r7", Cpu::CortexM4);
    check_thumb(
        "itte ge\nmovge r4, r5\nmovge r6, r7\nmovlt r8, r9",
        Cpu::CortexM4,
    );
}

#[test]
fn thumb2_branch_wide() {
    check_thumb("b.w target\ntarget:", Cpu::CortexM4);
}

#[test]
fn thumb2_ssat_usat() {
    check_thumb("ssat r5, #16, r8", Cpu::CortexM4);
    check_thumb("ssat r3, #8, r10", Cpu::CortexM4);
    check_thumb("usat r7, #16, r4", Cpu::CortexM4);
    check_thumb("usat r9, #24, r6", Cpu::CortexM4);
}

#[test]
fn thumb2_ldrd_strd() {
    check_thumb("ldrd r4, r5, [r8]", Cpu::CortexM4);
    check_thumb("ldrd r3, r7, [r9, #8]", Cpu::CortexM4);
    check_thumb("strd r6, r10, [r4]", Cpu::CortexM4);
    check_thumb("strd r8, r9, [r3, #8]", Cpu::CortexM4);
}

#[test]
fn thumb2_push_pop_wide() {
    check_thumb("push.w {r0, r1, r2, r3, r4, r5, r6, r7, r8}", Cpu::CortexM4);
    check_thumb("pop.w {r0, r1, r2, r3, r4, r5, r6, r7, r8}", Cpu::CortexM4);
}

#[test]
fn thumb2_mrs_msr() {
    check_thumb("mrs r0, APSR", Cpu::CortexM4);
    check_thumb("mrs r5, PRIMASK", Cpu::CortexM4);
    check_thumb("msr APSR_nzcvq, r3", Cpu::CortexM4);
    check_thumb("msr PRIMASK, r7", Cpu::CortexM4);
}

#[test]
fn thumb2_tbb_tbh() {
    check_thumb("tbb [pc, r3]", Cpu::CortexM4);
    check_thumb("tbb [r4, r7]", Cpu::CortexM4);
    check_thumb("tbh [pc, r5, lsl #1]", Cpu::CortexM4);
    check_thumb("tbh [r6, r8, lsl #1]", Cpu::CortexM4);
}

#[test]
fn thumb2_extend() {
    check_thumb("sxth.w r5, r8", Cpu::CortexM4);
    check_thumb("sxtb.w r3, r10", Cpu::CortexM4);
    check_thumb("uxth.w r7, r4", Cpu::CortexM4);
    check_thumb("uxtb.w r9, r6", Cpu::CortexM4);
}

#[test]
fn thumb2_extend_add() {
    check_thumb("sxtah r4, r7, r9", Cpu::CortexM4);
    check_thumb("sxtab r5, r8, r10", Cpu::CortexM4);
    check_thumb("uxtah r3, r6, r11", Cpu::CortexM4);
    check_thumb("uxtab r8, r4, r7", Cpu::CortexM4);
}

#[test]
fn thumb2_dsp_mul() {
    check_thumb("smulbb r4, r7, r9", Cpu::CortexM4);
    check_thumb("smulbt r5, r8, r3", Cpu::CortexM4);
    check_thumb("smultb r6, r10, r4", Cpu::CortexM4);
    check_thumb("smultt r3, r9, r7", Cpu::CortexM4);
    check_thumb("smmul r8, r5, r10", Cpu::CortexM4);
    check_thumb("smuad r4, r6, r3", Cpu::CortexM4);
    check_thumb("smusd r7, r9, r5", Cpu::CortexM4);
}

#[test]
fn thumb2_dsp_mul_acc() {
    check_thumb("smlabb r4, r7, r9, r3", Cpu::CortexM4);
    check_thumb("smmla r5, r8, r3, r10", Cpu::CortexM4);
    check_thumb("smmls r6, r9, r4, r11", Cpu::CortexM4);
    check_thumb("smlad r3, r10, r5, r7", Cpu::CortexM4);
    check_thumb("smlsd r8, r4, r6, r9", Cpu::CortexM4);
    check_thumb("usad8 r5, r7, r10", Cpu::CortexM4);
    check_thumb("usada8 r4, r8, r3, r6", Cpu::CortexM4);
}

#[test]
fn thumb2_dsp_long_mul() {
    check_thumb("smlalbb r4, r6, r8, r10", Cpu::CortexM4);
    check_thumb("smlalbt r3, r5, r7, r9", Cpu::CortexM4);
    check_thumb("smlaltb r2, r8, r4, r10", Cpu::CortexM4);
    check_thumb("smlaltt r5, r7, r9, r11", Cpu::CortexM4);
    check_thumb("smlald r4, r6, r8, r3", Cpu::CortexM4);
    check_thumb("smlsld r3, r5, r7, r10", Cpu::CortexM4);
}

#[test]
fn thumb2_parallel_arith() {
    check_thumb("sadd16 r4, r7, r9", Cpu::CortexM4);
    check_thumb("sadd8 r5, r8, r3", Cpu::CortexM4);
    check_thumb("ssub16 r6, r10, r4", Cpu::CortexM4);
    check_thumb("ssub8 r3, r9, r7", Cpu::CortexM4);
    check_thumb("uadd16 r8, r5, r10", Cpu::CortexM4);
    check_thumb("uadd8 r4, r6, r3", Cpu::CortexM4);
    check_thumb("usub16 r7, r9, r5", Cpu::CortexM4);
    check_thumb("usub8 r3, r8, r4", Cpu::CortexM4);
    check_thumb("qadd16 r5, r7, r10", Cpu::CortexM4);
    check_thumb("qadd8 r6, r3, r9", Cpu::CortexM4);
    check_thumb("sasx r4, r8, r5", Cpu::CortexM4);
    check_thumb("ssax r7, r10, r3", Cpu::CortexM4);
    check_thumb("uasx r9, r4, r6", Cpu::CortexM4);
    check_thumb("usax r3, r5, r8", Cpu::CortexM4);
}

#[test]
fn thumb2_sat_arith() {
    check_thumb("qadd r4, r7, r9", Cpu::CortexM4);
    check_thumb("qdadd r5, r8, r3", Cpu::CortexM4);
    check_thumb("qsub r6, r10, r4", Cpu::CortexM4);
    check_thumb("qdsub r3, r9, r7", Cpu::CortexM4);
}

#[test]
fn thumb2_packing() {
    check_thumb("pkhbt r4, r7, r9", Cpu::CortexM4);
    check_thumb("pkhbt r5, r8, r3, lsl #4", Cpu::CortexM4);
    check_thumb("sel r6, r10, r4", Cpu::CortexM4);
}

#[test]
fn thumb2_barriers() {
    check_thumb("dmb sy", Cpu::CortexM4);
    check_thumb("dsb sy", Cpu::CortexM4);
    check_thumb("isb sy", Cpu::CortexM4);
    // Barrier options
    check_thumb("dmb ish", Cpu::CortexM4);
    check_thumb("dmb ishst", Cpu::CortexM4);
    check_thumb("dmb nsh", Cpu::CortexM4);
    check_thumb("dmb nshst", Cpu::CortexM4);
    check_thumb("dmb osh", Cpu::CortexM4);
    check_thumb("dmb oshst", Cpu::CortexM4);
    check_thumb("dmb st", Cpu::CortexM4);
    check_thumb("dsb ish", Cpu::CortexM4);
    check_thumb("dsb ishst", Cpu::CortexM4);
    check_thumb("dsb st", Cpu::CortexM4);
    check_thumb("isb", Cpu::CortexM4);
}

#[test]
fn thumb2_rsb() {
    check_thumb("rsb.w r5, r8, #0", Cpu::CortexM4);
    check_thumb("rsb r4, r7, r10", Cpu::CortexM4);
}

#[test]
fn thumb2_mvn_wide() {
    check_thumb("mvn.w r5, r8", Cpu::CortexM4);
    check_thumb("mvn.w r3, r10", Cpu::CortexM4);
    check_thumb("mvn.w r7, #0xFF", Cpu::CortexM4);
}

#[test]
fn thumb2_neg_wide() {
    // NEG is alias for RSB Rd, Rn, #0
    check_thumb("rsb r5, r8, #0", Cpu::CortexM4);
}

#[test]
fn thumb2_ldrb_strb_wide() {
    check_thumb("ldrb.w r5, [r8]", Cpu::CortexM4);
    check_thumb("ldrb.w r3, [r9, #100]", Cpu::CortexM4);
    check_thumb("strb.w r7, [r4]", Cpu::CortexM4);
    check_thumb("strb.w r10, [r6, #100]", Cpu::CortexM4);
}

#[test]
fn thumb2_ldrh_strh_wide() {
    check_thumb("ldrh.w r5, [r8]", Cpu::CortexM4);
    check_thumb("ldrh.w r3, [r9, #100]", Cpu::CortexM4);
    check_thumb("strh.w r7, [r4]", Cpu::CortexM4);
    check_thumb("strh.w r10, [r6, #100]", Cpu::CortexM4);
}

#[test]
fn thumb2_ldm_stm_wide() {
    check_thumb("ldm.w r4, {r1, r2, r3, r8}", Cpu::CortexM4);
    check_thumb("stm.w r5!, {r1, r2, r3, r8}", Cpu::CortexM4);
    check_thumb("ldm.w r9!, {r0, r1, r2}", Cpu::CortexM4);
}

#[test]
fn thumb2_ldr_str_signed() {
    check_thumb("ldrsb.w r5, [r8]", Cpu::CortexM4);
    check_thumb("ldrsh.w r3, [r9]", Cpu::CortexM4);
    check_thumb("ldrsb.w r7, [r4, #10]", Cpu::CortexM4);
    check_thumb("ldrsh.w r10, [r6, #10]", Cpu::CortexM4);
}

#[test]
fn thumb2_clrex() {
    check_thumb("clrex", Cpu::CortexM4);
}

#[test]
fn thumb2_bl() {
    check_thumb("bl target\ntarget:", Cpu::CortexM4);
}

#[test]
fn thumb2_high_reg_ops() {
    // Operations on high registers require wide encoding
    check_thumb("add.w r8, r9, r10", Cpu::CortexM4);
    check_thumb("sub.w r10, r11, #1", Cpu::CortexM4);
    check_thumb("cmp.w r9, #0", Cpu::CortexM4);
    check_thumb("and.w r8, r10, r11", Cpu::CortexM4);
}

#[test]
fn thumb2_modified_imm_patterns() {
    // Test different modified immediate encoding patterns
    check_thumb("mov.w r5, #0x00FF00FF", Cpu::CortexM4); // pattern 01: 0x00XY00XY
    check_thumb("mov.w r8, #0xFF00FF00", Cpu::CortexM4); // pattern 10: 0xXY00XY00
    check_thumb("mov.w r3, #0xFFFFFFFF", Cpu::CortexM4); // pattern 11: 0xXYXYXYXY
    check_thumb("mov.w r10, #0x1F000000", Cpu::CortexM4); // rotated byte
}

// ---------------------------------------------------------------------------
// A32 additional instruction tests
// ---------------------------------------------------------------------------

#[test]
fn a32_adr() {
    check_a32("adr r5, target\nnop\ntarget:", Cpu::CortexA7);
}

#[test]
fn a32_neg() {
    check_a32("neg r4, r7", Cpu::CortexA7);
    check_a32("negs r8, r11", Cpu::CortexA7);
}

#[test]
fn a32_sxtb16_uxtb16() {
    check_a32("sxtb16 r5, r9", Cpu::CortexA7);
    check_a32("uxtb16 r8, r3", Cpu::CortexA7);
}

#[test]
fn a32_sxtab16_uxtab16() {
    check_a32("sxtab16 r4, r7, r10", Cpu::CortexA7);
    check_a32("uxtab16 r8, r2, r11", Cpu::CortexA7);
}

#[test]
fn a32_pld() {
    check_a32("pld [r5, #100]", Cpu::CortexA7);
    check_a32("pld [r9, #-32]", Cpu::CortexA7);
    check_a32("pld [r0]", Cpu::CortexA7);
}

#[test]
fn a32_pli() {
    check_a32("pli [r7, #64]", Cpu::CortexA7);
    check_a32("pli [r3, #-16]", Cpu::CortexA7);
}

#[test]
fn a32_ldrt_strt() {
    check_a32("ldrt r4, [r7]", Cpu::CortexA7);
    check_a32("strt r8, [r3]", Cpu::CortexA7);
    check_a32("ldrbt r5, [r9]", Cpu::CortexA7);
    check_a32("strbt r10, [r2]", Cpu::CortexA7);
}

#[test]
fn a32_ldrht_strht() {
    check_a32("ldrht r4, [r7]", Cpu::CortexA7);
    check_a32("strht r8, [r3]", Cpu::CortexA7);
    check_a32("ldrsbt r5, [r9]", Cpu::CortexA7);
    check_a32("ldrsht r10, [r2]", Cpu::CortexA7);
}

#[test]
fn a32_register_shift_dp() {
    check_a32("add r4, r7, r9, lsl r3", Cpu::CortexA7);
    check_a32("sub r8, r10, r2, asr r5", Cpu::CortexA7);
    check_a32("and r1, r6, r11, lsr r4", Cpu::CortexA7);
    check_a32("orr r3, r8, r5, ror r2", Cpu::CortexA7);
}

#[test]
fn a32_register_shift_mov() {
    check_a32("mov r4, r7, lsl r9", Cpu::CortexA7);
    check_a32("mov r8, r3, asr r5", Cpu::CortexA7);
}

#[test]
fn a32_register_shift_cmp() {
    check_a32("cmp r4, r7, lsl r9", Cpu::CortexA7);
    check_a32("tst r8, r3, asr r5", Cpu::CortexA7);
}

#[test]
fn a32_post_index_ldr_str() {
    check_a32("ldr r4, [r7], #8", Cpu::CortexA7);
    check_a32("str r8, [r3], #-4", Cpu::CortexA7);
    check_a32("ldrb r5, [r9], #1", Cpu::CortexA7);
}

#[test]
fn a32_pre_index_writeback() {
    check_a32("ldr r0, [r1, #4]!", Cpu::CortexA7);
    check_a32("str r2, [r3, #-8]!", Cpu::CortexA7);
    check_a32("ldrb r4, [r5, #1]!", Cpu::CortexA7);
    check_a32("strb r6, [r7, #-2]!", Cpu::CortexA7);
}

#[test]
fn a32_register_offset_post_index() {
    check_a32("ldr r4, [r5], r6", Cpu::CortexA7);
    check_a32("str r7, [r8], r9", Cpu::CortexA7);
}

#[test]
fn a32_negative_register_offset() {
    check_a32("ldr r7, [r8, -r9]", Cpu::CortexA7);
    check_a32("str r4, [r5, -r6]", Cpu::CortexA7);
}

#[test]
fn a32_rrx() {
    check_a32("rrx r0, r1", Cpu::CortexA7);
    check_a32("rrxs r5, r8", Cpu::CortexA7);
}

#[test]
fn a32_blx_label() {
    check_a32("blx target\ntarget:\nnop", Cpu::CortexA7);
}

#[test]
fn a32_cpsie_cpsid() {
    check_a32("cpsie if", Cpu::CortexA7);
    check_a32("cpsid if", Cpu::CortexA7);
    check_a32("cpsie i", Cpu::CortexA7);
    check_a32("cpsid a", Cpu::CortexA7);
    check_a32("cpsie aif", Cpu::CortexA7);
}

#[test]
fn a32_dbg() {
    check_a32("dbg #0", Cpu::CortexA7);
    check_a32("dbg #5", Cpu::CortexA7);
    check_a32("dbg #15", Cpu::CortexA7);
}

#[test]
fn a32_halfword_writeback() {
    check_a32("ldrh r0, [r1, #4]!", Cpu::CortexA7);
    check_a32("strh r2, [r3, #-6]!", Cpu::CortexA7);
    check_a32("ldrh r4, [r5], #8", Cpu::CortexA7);
    check_a32("strh r6, [r7], #-2", Cpu::CortexA7);
}

#[test]
fn a32_halfword_neg_reg() {
    check_a32("ldrh r0, [r1, -r2]", Cpu::CortexA7);
    check_a32("strh r3, [r4, -r5]", Cpu::CortexA7);
    check_a32("ldrsh r6, [r7, -r8]", Cpu::CortexA7);
    check_a32("ldrsb r9, [r10, -r11]", Cpu::CortexA7);
}

#[test]
fn a32_ldrd_strd_writeback() {
    check_a32("ldrd r4, r5, [r8, #8]!", Cpu::CortexA7);
    check_a32("strd r6, r7, [r3, #-16]!", Cpu::CortexA7);
    check_a32("ldrd r2, r3, [r9], #8", Cpu::CortexA7);
    check_a32("strd r0, r1, [r4], #-8", Cpu::CortexA7);
}

#[test]
fn a32_conditional_misc() {
    check_a32("addeq r0, r1, #5", Cpu::CortexA7);
    check_a32("subne r2, r3, r4", Cpu::CortexA7);
    check_a32("moveq r5, #42", Cpu::CortexA7);
}

#[test]
fn a32_mov_rrx_shift() {
    check_a32("mov r4, r9, rrx", Cpu::CortexA7);
}

#[test]
fn a32_pkhtb() {
    check_a32("pkhtb r5, r8, r3, asr #4", Cpu::CortexA7);
}

#[test]
fn a32_neg_shifted_reg_offset() {
    check_a32("ldr r0, [r1, -r2, lsl #3]", Cpu::CortexA7);
    check_a32("str r3, [r4, -r5, lsr #2]", Cpu::CortexA7);
}

#[test]
fn a32_ldm_stm_ib_da() {
    check_a32("ldmib r5!, {r0, r1, r2}", Cpu::CortexA7);
    check_a32("stmib r4!, {r0, r1, r2}", Cpu::CortexA7);
    check_a32("ldmda r8!, {r3, r4, r5}", Cpu::CortexA7);
    check_a32("stmda r9!, {r3, r4, r5}", Cpu::CortexA7);
}

#[test]
fn a32_barrier_options() {
    check_a32("dmb sy", Cpu::CortexA7);
    check_a32("dmb st", Cpu::CortexA7);
    check_a32("dmb ish", Cpu::CortexA7);
    check_a32("dmb ishst", Cpu::CortexA7);
    check_a32("dsb sy", Cpu::CortexA7);
    check_a32("dsb ish", Cpu::CortexA7);
    check_a32("isb sy", Cpu::CortexA7);
}

#[test]
fn a32_register_writeback() {
    check_a32("ldr r0, [r1, r2]!", Cpu::CortexA7);
    check_a32("str r3, [r4, -r5]!", Cpu::CortexA7);
    check_a32("ldrh r6, [r7, r8]!", Cpu::CortexA7);
    check_a32("strh r9, [r10, -r11]!", Cpu::CortexA7);
}

#[test]
fn a32_ldrt_strt_reg() {
    check_a32("ldrt r4, [r7], r2", Cpu::CortexA7);
    check_a32("strt r8, [r3], -r5", Cpu::CortexA7);
    check_a32("ldrt r4, [r7], r2, lsl #2", Cpu::CortexA7);
}

#[test]
fn a32_post_index_reg() {
    // Post-index with register and shifted register
    check_a32("ldr r0, [r1], r2", Cpu::CortexA7);
    check_a32("str r3, [r4], -r5", Cpu::CortexA7);
    check_a32("ldr r6, [r7], r8, lsl #2", Cpu::CortexA7);
    check_a32("str r9, [r10], -r11, asr #3", Cpu::CortexA7);
}

#[test]
fn a32_ldr_literal() {
    // PC-relative loads with labels
    check_a32("ldr r0, target\ntarget:\n.word 0x12345678", Cpu::CortexA7);
}

#[test]
fn a32_shifted_writeback() {
    // Pre-index shifted register with writeback
    check_a32("ldr r0, [r1, r2, lsl #2]!", Cpu::CortexA7);
    check_a32("str r3, [r4, r5, lsr #1]!", Cpu::CortexA7);
}

#[test]
fn a32_adr_backward() {
    check_a32("nop\nadr r0, back\nback:\nnop", Cpu::CortexA7);
}

#[test]
fn a32_conditional_blx_reg() {
    check_a32("blxne r5", Cpu::CortexA7);
}

#[test]
fn a32_conditional_ldrex() {
    check_a32("ldrexeq r0, [r1]", Cpu::CortexA7);
}

#[test]
fn a32_conditional_svc() {
    check_a32("svceq #0", Cpu::CortexA7);
}

#[test]
fn a32_bkpt_large() {
    check_a32("bkpt #255", Cpu::CortexA7);
}

#[test]
fn a32_rotated_immediates() {
    check_a32("and r0, r1, #0xFF000000", Cpu::CortexA7);
    check_a32("orr r2, r3, #0x00FF0000", Cpu::CortexA7);
    check_a32("eor r4, r5, #0xFF00", Cpu::CortexA7);
    check_a32("mov r6, #0xC000003F", Cpu::CortexA7);
}

#[test]
fn a32_movw_movt_full() {
    check_a32("movw r0, #0xABCD", Cpu::CortexA7);
    check_a32("movt r0, #0x1234", Cpu::CortexA7);
}

#[test]
fn a32_ldr_str_large_offset() {
    check_a32("ldr r0, [r1, #4095]", Cpu::CortexA7);
    check_a32("str r2, [r3, #-4095]", Cpu::CortexA7);
    check_a32("ldrh r4, [r5, #255]", Cpu::CortexA7);
    check_a32("strh r6, [r7, #-255]", Cpu::CortexA7);
}

#[test]
fn a32_adc_sbc_rsc() {
    check_a32("adc r0, r1, #5", Cpu::CortexA7);
    check_a32("sbc r2, r3, r4", Cpu::CortexA7);
    check_a32("rsc r5, r6, #10", Cpu::CortexA7);
    check_a32("adcs r7, r8, r9", Cpu::CortexA7);
    check_a32("sbcs r10, r11, #1", Cpu::CortexA7);
}

#[test]
fn a32_cmn() {
    check_a32("cmn r0, #5", Cpu::CortexA7);
    check_a32("cmn r1, r2", Cpu::CortexA7);
    check_a32("cmn r3, r4, lsl #2", Cpu::CortexA7);
}

#[test]
fn a32_conditional_data_proc() {
    check_a32("addeq r0, r1, r2", Cpu::CortexA7);
    check_a32("subne r3, r4, #10", Cpu::CortexA7);
    check_a32("andgt r5, r6, r7", Cpu::CortexA7);
    check_a32("orrle r8, r9, #0xFF", Cpu::CortexA7);
    check_a32("eorcs r10, r11, r12", Cpu::CortexA7);
    check_a32("biccc r0, r1, #0xF0", Cpu::CortexA7);
}

#[test]
fn a32_conditional_ldr_str() {
    check_a32("ldreq r0, [r1, #4]", Cpu::CortexA7);
    check_a32("strne r2, [r3]", Cpu::CortexA7);
    check_a32("ldrbeq r4, [r5, #1]", Cpu::CortexA7);
    check_a32("ldrheq r6, [r7, #2]", Cpu::CortexA7);
}

#[test]
fn a32_conditional_branch() {
    check_a32("bgt target\nnop\ntarget:\nnop", Cpu::CortexA7);
    check_a32("ble target\nnop\ntarget:\nnop", Cpu::CortexA7);
    check_a32("bls target\nnop\ntarget:\nnop", Cpu::CortexA7);
    check_a32("bhi target\nnop\ntarget:\nnop", Cpu::CortexA7);
}

#[test]
fn a32_mul_set_flags() {
    check_a32("muls r4, r7, r9", Cpu::CortexA7);
    check_a32("mlas r5, r8, r3, r10", Cpu::CortexA7);
}

#[test]
fn a32_mvn_imm() {
    check_a32("mvn r0, #0", Cpu::CortexA7);
    check_a32("mvn r1, #0xFF", Cpu::CortexA7);
}

#[test]
fn a32_neg_set_flags() {
    check_a32("negs r0, r1", Cpu::CortexA7);
}

#[test]
fn a32_rrx_in_dp() {
    // RRX as shift operand in data processing
    check_a32("add r0, r1, r2, rrx", Cpu::CortexA7);
    check_a32("sub r3, r4, r5, rrx", Cpu::CortexA7);
    check_a32("cmp r6, r7, rrx", Cpu::CortexA7);
}

#[test]
fn a32_extend_with_rotation() {
    check_a32("sxth r0, r1, ror #8", Cpu::CortexA7);
    check_a32("uxtb r2, r3, ror #16", Cpu::CortexA7);
    check_a32("sxtb r4, r5, ror #24", Cpu::CortexA7);
    check_a32("uxth r6, r7, ror #8", Cpu::CortexA7);
}

#[test]
fn a32_extend_add_with_rotation() {
    check_a32("sxtah r0, r1, r2, ror #8", Cpu::CortexA7);
    check_a32("uxtab r3, r4, r5, ror #16", Cpu::CortexA7);
}

#[test]
fn a32_realistic_memcpy() {
    let src = "\
        push {r4, lr}
        mov r4, r2
    loop:
        subs r4, r4, #1
        ldrb r3, [r1], #1
        strb r3, [r0], #1
        bne loop
        pop {r4, pc}
    ";
    check_a32(src, Cpu::CortexA7);
}

#[test]
fn a32_ssat_with_shift() {
    check_a32("ssat r0, #16, r1, lsl #4", Cpu::CortexA7);
    check_a32("usat r2, #8, r3, asr #7", Cpu::CortexA7);
}

#[test]
fn a32_realistic_atomic_add() {
    let src = "\
    retry:
        ldrex r2, [r0]
        add r2, r2, r1
        strex r3, r2, [r0]
        cmp r3, #0
        bne retry
        bx lr
    ";
    check_a32(src, Cpu::CortexA7);
}

// --- DSP multiply variants ---

#[test]
fn a32_smlaxy_variants() {
    check_a32("smlabb r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlabt r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlatb r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlatt r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smulxy_variants() {
    check_a32("smulbb r0, r1, r2", Cpu::CortexA7);
    check_a32("smulbt r0, r1, r2", Cpu::CortexA7);
    check_a32("smultb r0, r1, r2", Cpu::CortexA7);
    check_a32("smultt r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_smlalxy_variants() {
    check_a32("smlalbb r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlalbt r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlaltb r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlaltt r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smlsld() {
    check_a32("smlsld r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smuad_smusd() {
    check_a32("smuad r0, r1, r2", Cpu::CortexA7);
    check_a32("smusd r0, r1, r2", Cpu::CortexA7);
    check_a32("smlad r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlsd r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlald r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smmul_smmla_smmls() {
    check_a32("smmul r0, r1, r2", Cpu::CortexA7);
    check_a32("smmla r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smmls r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_usad8_usada8() {
    check_a32("usad8 r0, r1, r2", Cpu::CortexA7);
    check_a32("usada8 r0, r1, r2, r3", Cpu::CortexA7);
}

// --- Parallel arithmetic (signed) ---

#[test]
fn a32_parallel_signed() {
    check_a32("sadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("sadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("ssub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("ssub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("sasx r0, r1, r2", Cpu::CortexA7);
    check_a32("ssax r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_parallel_unsigned() {
    check_a32("uadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("usub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("usub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("uasx r0, r1, r2", Cpu::CortexA7);
    check_a32("usax r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_parallel_saturating() {
    check_a32("qadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("qadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("qsub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("qsub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("qasx r0, r1, r2", Cpu::CortexA7);
    check_a32("qsax r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_parallel_signed_halving() {
    check_a32("shadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("shadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("shsub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("shsub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("shasx r0, r1, r2", Cpu::CortexA7);
    check_a32("shsax r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_parallel_unsigned_saturating() {
    check_a32("uqadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uqadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("uqsub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uqsub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("uqasx r0, r1, r2", Cpu::CortexA7);
    check_a32("uqsax r0, r1, r2", Cpu::CortexA7);
}

#[test]
fn a32_parallel_unsigned_halving() {
    check_a32("uhadd16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uhadd8 r0, r1, r2", Cpu::CortexA7);
    check_a32("uhsub16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uhsub8 r0, r1, r2", Cpu::CortexA7);
    check_a32("uhasx r0, r1, r2", Cpu::CortexA7);
    check_a32("uhsax r0, r1, r2", Cpu::CortexA7);
}

// --- Exclusive load/store ---

#[test]
fn a32_exclusive_byte_halfword() {
    check_a32("ldrexb r0, [r1]", Cpu::CortexA7);
    check_a32("strexb r0, r1, [r2]", Cpu::CortexA7);
    check_a32("ldrexh r0, [r1]", Cpu::CortexA7);
    check_a32("strexh r0, r1, [r2]", Cpu::CortexA7);
    check_a32("clrex", Cpu::CortexA7);
}

// --- Bit manipulation ---

#[test]
fn a32_bfi_bfc() {
    check_a32("bfi r0, r1, #4, #8", Cpu::CortexA7);
    check_a32("bfc r0, #0, #16", Cpu::CortexA7);
}

#[test]
fn a32_ubfx_sbfx() {
    check_a32("ubfx r0, r1, #8, #4", Cpu::CortexA7);
    check_a32("sbfx r2, r3, #0, #16", Cpu::CortexA7);
}

// --- Byte reversal ---

#[test]
fn a32_rev_rev16_revsh() {
    check_a32("rev r0, r1", Cpu::CortexA7);
    check_a32("rev16 r2, r3", Cpu::CortexA7);
    check_a32("revsh r4, r5", Cpu::CortexA7);
}

// --- CLZ / RBIT ---

#[test]
fn a32_clz_rbit_regs() {
    check_a32("clz r0, r1", Cpu::CortexA7);
    check_a32("rbit r2, r3", Cpu::CortexA7);
}

// --- Extend (plain, no rotation) ---

#[test]
fn a32_extend_plain() {
    check_a32("sxth r0, r1", Cpu::CortexA7);
    check_a32("sxtb r0, r1", Cpu::CortexA7);
    check_a32("uxth r0, r1", Cpu::CortexA7);
    check_a32("uxtb r0, r1", Cpu::CortexA7);
    check_a32("sxtb16 r0, r1", Cpu::CortexA7);
    check_a32("uxtb16 r0, r1", Cpu::CortexA7);
}

// --- Extend and add (plain) ---

#[test]
fn a32_extend_add_plain() {
    check_a32("sxtab r0, r1, r2", Cpu::CortexA7);
    check_a32("sxtah r0, r1, r2", Cpu::CortexA7);
    check_a32("uxtab r0, r1, r2", Cpu::CortexA7);
    check_a32("uxtah r0, r1, r2", Cpu::CortexA7);
    check_a32("sxtab16 r0, r1, r2", Cpu::CortexA7);
    check_a32("uxtab16 r0, r1, r2", Cpu::CortexA7);
}

// --- Saturating arithmetic ---

#[test]
fn a32_qadd_qsub() {
    check_a32("qadd r0, r1, r2", Cpu::CortexA7);
    check_a32("qdadd r0, r1, r2", Cpu::CortexA7);
    check_a32("qsub r0, r1, r2", Cpu::CortexA7);
    check_a32("qdsub r0, r1, r2", Cpu::CortexA7);
}

// --- Packing ---

#[test]
fn a32_pkhbt_pkhtb_sel() {
    check_a32("pkhbt r0, r1, r2", Cpu::CortexA7);
    check_a32("pkhbt r0, r1, r2, lsl #16", Cpu::CortexA7);
    check_a32("pkhtb r0, r1, r2, asr #16", Cpu::CortexA7);
    check_a32("sel r0, r1, r2", Cpu::CortexA7);
}

// --- Unprivileged load/store ---

#[test]
fn a32_ldrt_strt_variants() {
    check_a32("ldrt r0, [r1]", Cpu::CortexA7);
    check_a32("strt r0, [r1]", Cpu::CortexA7);
    check_a32("ldrbt r0, [r1]", Cpu::CortexA7);
    check_a32("strbt r0, [r1]", Cpu::CortexA7);
    check_a32("ldrht r0, [r1]", Cpu::CortexA7);
    check_a32("strht r0, [r1]", Cpu::CortexA7);
    check_a32("ldrsbt r0, [r1]", Cpu::CortexA7);
    check_a32("ldrsht r0, [r1]", Cpu::CortexA7);
}

// --- System instructions ---

#[test]
fn a32_wfi_wfe_sev() {
    check_a32("wfi", Cpu::CortexA7);
    check_a32("wfe", Cpu::CortexA7);
    check_a32("sev", Cpu::CortexA7);
}

#[test]
fn a32_mrs_msr_apsr() {
    check_a32("mrs r0, apsr", Cpu::CortexA7);
    check_a32("msr apsr_nzcvq, r0", Cpu::CortexA7);
}

// --- LDRD / STRD ---

#[test]
fn a32_ldrd_strd_imm() {
    check_a32("ldrd r0, r1, [r2]", Cpu::CortexA7);
    check_a32("ldrd r0, r1, [r2, #8]", Cpu::CortexA7);
    check_a32("strd r0, r1, [r2]", Cpu::CortexA7);
    check_a32("strd r0, r1, [r2, #-8]", Cpu::CortexA7);
}

// --- PLD / PLI ---

#[test]
fn a32_pld_pli() {
    check_a32("pld [r0]", Cpu::CortexA7);
    check_a32("pld [r0, #64]", Cpu::CortexA7);
    check_a32("pli [r0]", Cpu::CortexA7);
    check_a32("pli [r0, #32]", Cpu::CortexA7);
}

// ===========================================================================
// Systematic register coverage tests
// ===========================================================================

/// Interesting register indices for systematic testing.
/// Covers low regs (0,1,5,7), the narrow/wide Thumb boundary (7→8),
/// high regs (8,12), and LR (14). Excludes SP(13) and PC(15) which
/// have special restrictions in most instructions.
const TEST_REGS: &[u8] = &[0, 1, 5, 7, 8, 12, 14];

/// Smaller set for 4-operand instructions to keep runtime reasonable.
const TEST_REGS_4OP: &[u8] = &[0, 5, 8, 12];

fn rn(r: u8) -> String {
    format!("r{r}")
}

/// Iterate all distinct pairs from `regs`.
fn test_2_args(regs: &[u8], mut f: impl FnMut(u8, u8)) {
    for &a in regs {
        for &b in regs {
            if a != b {
                f(a, b);
            }
        }
    }
}

/// Iterate all distinct triples from `regs`.
fn test_3_args(regs: &[u8], mut f: impl FnMut(u8, u8, u8)) {
    for &a in regs {
        for &b in regs {
            for &c in regs {
                if a != b && a != c && b != c {
                    f(a, b, c);
                }
            }
        }
    }
}

/// Iterate all distinct quads from `regs`.
fn test_4_args(regs: &[u8], mut f: impl FnMut(u8, u8, u8, u8)) {
    for &a in regs {
        for &b in regs {
            for &c in regs {
                for &d in regs {
                    if a != b && a != c && a != d && b != c && b != d && c != d {
                        f(a, b, c, d);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// A32: Systematic register tests
// ---------------------------------------------------------------------------

#[test]
fn a32_regs_2op_unary() {
    for mn in ["rev", "rev16", "revsh", "clz", "rbit"] {
        test_2_args(TEST_REGS, |rd, rm| {
            check_a32(&format!("{mn} {}, {}", rn(rd), rn(rm)), Cpu::CortexA7);
        });
    }
}

#[test]
fn a32_regs_2op_extend() {
    for mn in ["sxth", "sxtb", "uxth", "uxtb", "sxtb16", "uxtb16"] {
        test_2_args(TEST_REGS, |rd, rm| {
            check_a32(&format!("{mn} {}, {}", rn(rd), rn(rm)), Cpu::CortexA7);
        });
    }
}

#[test]
fn a32_regs_2op_mov() {
    for mn in ["mov", "mvn"] {
        test_2_args(TEST_REGS, |rd, rm| {
            check_a32(&format!("{mn} {}, {}", rn(rd), rn(rm)), Cpu::CortexA7);
        });
    }
}

#[test]
fn a32_regs_2op_test() {
    for mn in ["cmp", "cmn", "tst", "teq"] {
        test_2_args(TEST_REGS, |rn_reg, rm| {
            check_a32(&format!("{mn} {}, {}", rn(rn_reg), rn(rm)), Cpu::CortexA7);
        });
    }
}

#[test]
fn a32_regs_3op_dp() {
    for mn in [
        "add", "sub", "and", "orr", "eor", "bic", "adc", "sbc", "rsb", "rsc",
    ] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_shift() {
    for mn in ["lsl", "lsr", "asr", "ror"] {
        test_3_args(TEST_REGS, |rd, rm, rs| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rm), rn(rs)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_mul() {
    for mn in ["mul", "smmul", "smuad", "smusd", "usad8"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_dsp_mul() {
    for mn in ["smulbb", "smulbt", "smultb", "smultt"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_div() {
    for mn in ["sdiv", "udiv"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_parallel() {
    for mn in [
        "sadd16", "sadd8", "ssub16", "ssub8", "uadd16", "uadd8", "usub16", "usub8", "qadd16",
        "qadd8", "qsub16", "qsub8", "shadd16", "shadd8", "shsub16", "shsub8", "uhadd16", "uhadd8",
        "uhsub16", "uhsub8", "uqadd16", "uqadd8", "uqsub16", "uqsub8", "sasx", "ssax", "uasx",
        "usax", "qasx", "qsax", "shasx", "shsax", "uhasx", "uhsax", "uqasx", "uqsax",
    ] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_sat_arith() {
    for mn in ["qadd", "qdadd", "qsub", "qdsub"] {
        test_3_args(TEST_REGS, |rd, rm, rn_reg| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rm), rn(rn_reg)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_extend_add() {
    for mn in ["sxtab", "sxtah", "uxtab", "uxtah", "sxtab16", "uxtab16"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_3op_packing() {
    for mn in ["sel", "pkhbt"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_a32(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_4op_mla() {
    for mn in [
        "mla", "mls", "smmla", "smmls", "smlad", "smlsd", "usada8", "smlabb", "smlabt", "smlatb",
        "smlatt",
    ] {
        test_4_args(TEST_REGS_4OP, |rd, rn_reg, rm, ra| {
            check_a32(
                &format!("{mn} {}, {}, {}, {}", rn(rd), rn(rn_reg), rn(rm), rn(ra)),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_4op_long_mul() {
    for mn in [
        "umull", "smull", "umlal", "smlal", "smlalbb", "smlalbt", "smlaltb", "smlaltt", "smlald",
        "smlsld",
    ] {
        test_4_args(TEST_REGS_4OP, |rdlo, rdhi, rn_reg, rm| {
            check_a32(
                &format!(
                    "{mn} {}, {}, {}, {}",
                    rn(rdlo),
                    rn(rdhi),
                    rn(rn_reg),
                    rn(rm)
                ),
                Cpu::CortexA7,
            );
        });
    }
}

#[test]
fn a32_regs_ldr_str_reg() {
    for mn in ["ldr", "ldrb", "ldrh", "str", "strb", "strh"] {
        test_3_args(TEST_REGS, |rt, base, rm| {
            check_a32(
                &format!("{mn} {}, [{}, {}]", rn(rt), rn(base), rn(rm)),
                Cpu::CortexA7,
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Thumb: Systematic register tests
// ---------------------------------------------------------------------------

#[test]
fn thumb_regs_2op_unary() {
    for mn in ["rev", "rev16", "revsh", "sxth", "sxtb", "uxth", "uxtb"] {
        test_2_args(TEST_REGS, |rd, rm| {
            check_thumb(&format!("{mn} {}, {}", rn(rd), rn(rm)), Cpu::CortexM4);
        });
    }
}

#[test]
fn thumb_regs_2op_mov() {
    // MOV (no flags) works with any regs via Format 5
    test_2_args(TEST_REGS, |rd, rm| {
        check_thumb(&format!("mov {}, {}", rn(rd), rn(rm)), Cpu::CortexM4);
    });
}

#[test]
fn thumb_regs_2op_test() {
    for mn in ["cmp", "cmn", "tst"] {
        test_2_args(TEST_REGS, |rn_reg, rm| {
            check_thumb(&format!("{mn}.w {}, {}", rn(rn_reg), rn(rm)), Cpu::CortexM4);
        });
    }
}

#[test]
fn thumb_regs_3op_dp() {
    for mn in ["add", "sub", "and", "orr", "eor", "bic", "adc", "sbc"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn}.w {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_dp_narrow_collapse() {
    // Test that 3-reg form with Rd==Rn produces correct narrow encoding
    for mn in ["and", "orr", "eor", "bic", "adc", "sbc"] {
        for &rd in &[0u8, 1, 5, 7] {
            for &rm in &[0u8, 1, 5, 7] {
                if rd != rm {
                    check_thumb(
                        &format!("{mn}s {}, {}, {}", rn(rd), rn(rd), rn(rm)),
                        Cpu::CortexM4,
                    );
                }
            }
        }
    }
}

#[test]
fn thumb_regs_3op_shift() {
    for mn in ["lsl", "lsr", "asr"] {
        test_3_args(TEST_REGS, |rd, rm, rs| {
            check_thumb(
                &format!("{mn}.w {}, {}, {}", rn(rd), rn(rm), rn(rs)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_mul() {
    for mn in ["mul", "smmul", "smuad", "smusd", "usad8"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_dsp_mul() {
    for mn in ["smulbb", "smulbt", "smultb", "smultt"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_div() {
    for mn in ["sdiv", "udiv"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_parallel() {
    for mn in [
        "sadd16", "sadd8", "ssub16", "ssub8", "uadd16", "uadd8", "usub16", "usub8", "qadd16",
        "qadd8", "qsub16", "qsub8", "shadd16", "shadd8", "shsub16", "shsub8", "uhadd16", "uhadd8",
        "uhsub16", "uhsub8", "uqadd16", "uqadd8", "uqsub16", "uqsub8", "sasx", "ssax", "uasx",
        "usax", "qasx", "qsax", "shasx", "shsax", "uhasx", "uhsax", "uqasx", "uqsax",
    ] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_sat_arith() {
    for mn in ["qadd", "qdadd", "qsub", "qdsub"] {
        test_3_args(TEST_REGS, |rd, rm, rn_reg| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rm), rn(rn_reg)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_extend_add() {
    for mn in ["sxtab", "sxtah", "uxtab", "uxtah", "sxtab16", "uxtab16"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_3op_packing() {
    for mn in ["sel", "pkhbt"] {
        test_3_args(TEST_REGS, |rd, rn_reg, rm| {
            check_thumb(
                &format!("{mn} {}, {}, {}", rn(rd), rn(rn_reg), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_2op_clz_rbit() {
    for mn in ["clz", "rbit"] {
        test_2_args(TEST_REGS, |rd, rm| {
            check_thumb(&format!("{mn} {}, {}", rn(rd), rn(rm)), Cpu::CortexM4);
        });
    }
}

#[test]
fn thumb_regs_4op_mla() {
    for mn in [
        "mla", "mls", "smmla", "smmls", "smlad", "smlsd", "usada8", "smlabb", "smlabt", "smlatb",
        "smlatt",
    ] {
        test_4_args(TEST_REGS_4OP, |rd, rn_reg, rm, ra| {
            check_thumb(
                &format!("{mn} {}, {}, {}, {}", rn(rd), rn(rn_reg), rn(rm), rn(ra)),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_4op_long_mul() {
    for mn in [
        "umull", "smull", "umlal", "smlal", "smlalbb", "smlalbt", "smlaltb", "smlaltt", "smlald",
        "smlsld",
    ] {
        test_4_args(TEST_REGS_4OP, |rdlo, rdhi, rn_reg, rm| {
            check_thumb(
                &format!(
                    "{mn} {}, {}, {}, {}",
                    rn(rdlo),
                    rn(rdhi),
                    rn(rn_reg),
                    rn(rm)
                ),
                Cpu::CortexM4,
            );
        });
    }
}

#[test]
fn thumb_regs_ldr_str_reg() {
    for mn in ["ldr", "ldrb", "ldrh", "str", "strb", "strh"] {
        test_3_args(TEST_REGS, |rt, base, rm| {
            check_thumb(
                &format!("{mn}.w {}, [{}, {}]", rn(rt), rn(base), rn(rm)),
                Cpu::CortexM4,
            );
        });
    }
}

// ---------------------------------------------------------------------------
// Bug-targeted tests
// ---------------------------------------------------------------------------

#[test]
fn thumb_pld_pli_negative_offset() {
    check_thumb("pld [r0, #-32]", Cpu::CortexM4);
    check_thumb("pld [r5, #-1]", Cpu::CortexM4);
    check_thumb("pld [r0, #-255]", Cpu::CortexM4);
    check_thumb("pli [r0, #-16]", Cpu::CortexM4);
    check_thumb("pli [r3, #-128]", Cpu::CortexM4);
}

#[test]
fn thumb_pld_pli_positive_offset() {
    check_thumb("pld [r0]", Cpu::CortexM4);
    check_thumb("pld [r5, #100]", Cpu::CortexM4);
    check_thumb("pld [r0, #4095]", Cpu::CortexM4);
    check_thumb("pli [r0]", Cpu::CortexM4);
    check_thumb("pli [r7, #64]", Cpu::CortexM4);
}

#[test]
fn thumb_misc_high_regs() {
    // These require wide encoding (was silently truncating high regs before fix)
    for mn in ["rev", "rev16", "revsh", "sxth", "sxtb", "uxth", "uxtb"] {
        check_thumb(&format!("{mn} r8, r9"), Cpu::CortexM4);
        check_thumb(&format!("{mn} r0, r8"), Cpu::CortexM4);
        check_thumb(&format!("{mn} r12, r0"), Cpu::CortexM4);
    }
}

#[test]
fn thumb_alu_narrow_collapse() {
    // 3-reg form where Rd==Rn should produce narrow encoding
    check_thumb("ands r0, r0, r1", Cpu::CortexM4);
    check_thumb("orrs r3, r3, r5", Cpu::CortexM4);
    check_thumb("eors r2, r2, r4", Cpu::CortexM4);
    check_thumb("bics r1, r1, r6", Cpu::CortexM4);
    check_thumb("adcs r0, r0, r3", Cpu::CortexM4);
    check_thumb("sbcs r2, r2, r7", Cpu::CortexM4);
}

#[test]
fn thumb_adc_sbc_narrow() {
    // ADC/SBC narrow 2-reg form
    check_thumb("adcs r0, r1", Cpu::CortexM4);
    check_thumb("adcs r3, r5", Cpu::CortexM4);
    check_thumb("sbcs r2, r4", Cpu::CortexM4);
    check_thumb("sbcs r7, r0", Cpu::CortexM4);
}

#[test]
fn thumb_push_pop_high_regs() {
    // PUSH/POP with high registers require wide encoding
    check_thumb("push {r4-r11, lr}", Cpu::CortexM4);
    check_thumb("pop {r4-r11, pc}", Cpu::CortexM4);
    check_thumb("push {r4-r8, lr}", Cpu::CortexM4);
    check_thumb("pop {r4-r8, pc}", Cpu::CortexM4);
}

#[test]
fn thumb_neg_regs() {
    // NEGS (RSBS Rd, Rm, #0) - narrow for low regs
    check_thumb("negs r0, r1", Cpu::CortexM4);
    check_thumb("negs r7, r5", Cpu::CortexM4);
    // NEG with high regs needs wide encoding
    check_thumb("neg.w r8, r0", Cpu::CortexM4);
    check_thumb("neg.w r0, r8", Cpu::CortexM4);
}

// ---------------------------------------------------------------------------
// Boundary value tests
// ---------------------------------------------------------------------------

#[test]
fn a32_imm_boundaries() {
    // Maximum encodable immediate values
    check_a32("mov r0, #0", Cpu::CortexA7);
    check_a32("mov r0, #255", Cpu::CortexA7);
    check_a32("mov r0, #0xFF00", Cpu::CortexA7);
    check_a32("mov r0, #0xFF000000", Cpu::CortexA7);
    // Large rotated immediates
    check_a32("add r0, r1, #0x3FC00", Cpu::CortexA7);
}

#[test]
fn thumb_imm_boundaries() {
    // Narrow immediate boundaries
    check_thumb("movs r0, #0", Cpu::CortexM4);
    check_thumb("movs r0, #255", Cpu::CortexM4);
    // Wide immediate patterns
    check_thumb("mov.w r0, #256", Cpu::CortexM4);
    check_thumb("mov.w r0, #0x00FF00FF", Cpu::CortexM4);
    check_thumb("mov.w r0, #0xFF00FF00", Cpu::CortexM4);
    check_thumb("mov.w r0, #0xFFFFFFFF", Cpu::CortexM4);
}

#[test]
fn thumb_ldr_str_imm_boundaries() {
    // Narrow LDR/STR boundary: max word offset = 124
    check_thumb("ldr r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldr r0, [r1, #124]", Cpu::CortexM4);
    // Beyond narrow → wide
    check_thumb("ldr r0, [r1, #128]", Cpu::CortexM4);
    check_thumb("ldr r0, [r1, #4095]", Cpu::CortexM4);
    // Narrow byte: max 31
    check_thumb("ldrb r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldrb r0, [r1, #31]", Cpu::CortexM4);
    check_thumb("ldrb r0, [r1, #32]", Cpu::CortexM4);
    // Narrow half: max 62
    check_thumb("ldrh r0, [r1, #0]", Cpu::CortexM4);
    check_thumb("ldrh r0, [r1, #62]", Cpu::CortexM4);
    check_thumb("ldrh r0, [r1, #64]", Cpu::CortexM4);
    // SP-relative: max 1020
    check_thumb("ldr r0, [sp, #0]", Cpu::CortexM4);
    check_thumb("ldr r0, [sp, #1020]", Cpu::CortexM4);
    check_thumb("ldr r0, [sp, #1024]", Cpu::CortexM4);
    // Negative offset → wide
    check_thumb("ldr r0, [r1, #-1]", Cpu::CortexM4);
    check_thumb("str r0, [r1, #-255]", Cpu::CortexM4);
}

#[test]
fn a32_ldr_str_imm_boundaries() {
    check_a32("ldr r0, [r1, #0]", Cpu::CortexA7);
    check_a32("ldr r0, [r1, #4095]", Cpu::CortexA7);
    check_a32("ldr r0, [r1, #-4095]", Cpu::CortexA7);
    check_a32("ldrh r0, [r1, #0]", Cpu::CortexA7);
    check_a32("ldrh r0, [r1, #255]", Cpu::CortexA7);
    check_a32("ldrh r0, [r1, #-255]", Cpu::CortexA7);
}

#[test]
fn a32_shift_imm_boundaries() {
    // Shift amount boundaries
    check_a32("lsl r0, r1, #0", Cpu::CortexA7);
    check_a32("lsl r0, r1, #1", Cpu::CortexA7);
    check_a32("lsl r0, r1, #31", Cpu::CortexA7);
    check_a32("lsr r0, r1, #1", Cpu::CortexA7);
    check_a32("lsr r0, r1, #32", Cpu::CortexA7);
    check_a32("asr r0, r1, #1", Cpu::CortexA7);
    check_a32("asr r0, r1, #32", Cpu::CortexA7);
    check_a32("ror r0, r1, #1", Cpu::CortexA7);
    check_a32("ror r0, r1, #31", Cpu::CortexA7);
}

#[test]
fn thumb_shift_imm_boundaries() {
    check_thumb("lsls r0, r1, #0", Cpu::CortexM4);
    check_thumb("lsls r0, r1, #1", Cpu::CortexM4);
    check_thumb("lsls r0, r1, #31", Cpu::CortexM4);
    check_thumb("lsrs r0, r1, #1", Cpu::CortexM4);
    check_thumb("lsrs r0, r1, #32", Cpu::CortexM4);
    check_thumb("asrs r0, r1, #1", Cpu::CortexM4);
    check_thumb("asrs r0, r1, #32", Cpu::CortexM4);
}

#[test]
fn thumb_add_sub_imm_boundaries() {
    // Narrow ADD imm3: 0-7
    check_thumb("adds r0, r1, #0", Cpu::CortexM4);
    check_thumb("adds r0, r1, #7", Cpu::CortexM4);
    // Narrow ADD imm8: 0-255
    check_thumb("adds r0, #0", Cpu::CortexM4);
    check_thumb("adds r0, #255", Cpu::CortexM4);
    // Wide: larger immediates
    check_thumb("add.w r0, r1, #256", Cpu::CortexM4);
}

// ---------------------------------------------------------------------------
// Multi-instruction / integration tests
// ---------------------------------------------------------------------------

#[test]
fn thumb_realistic_isr() {
    check_thumb(
        "push {r4-r7, lr}\n\
         mov r4, r0\n\
         ldr r0, [r4, #0]\n\
         adds r0, #1\n\
         str r0, [r4, #0]\n\
         pop {r4-r7, pc}",
        Cpu::CortexM4,
    );
}

#[test]
fn a32_realistic_loop() {
    check_a32(
        "mov r0, #0\n\
         mov r1, #10\n\
         loop:\n\
         add r0, r0, #1\n\
         cmp r0, r1\n\
         blt loop",
        Cpu::CortexA7,
    );
}

// ---------------------------------------------------------------------------
// Bugs found via snippet comparison testing
// ---------------------------------------------------------------------------

// A32 PUSH/POP single register: should use STR/LDR encoding, not STMDB/LDMIA
#[test]
fn a32_push_single_register() {
    check_a32("push {r1}", Cpu::CortexA7);
    check_a32("push {r0}", Cpu::CortexA7);
    check_a32("push {lr}", Cpu::CortexA7);
    check_a32("pop {r1}", Cpu::CortexA7);
    check_a32("pop {r0}", Cpu::CortexA7);
    check_a32("pop {pc}", Cpu::CortexA7);
}

#[test]
fn a32_push_multi_register() {
    // Multi-register should still use STMDB/LDMIA
    check_a32("push {r0-r12, r14}", Cpu::CortexA7);
    check_a32("pop {r0-r12, r14}", Cpu::CortexA7);
    check_a32("push {r1, r12}", Cpu::CortexA7);
    check_a32("pop {r1, r12}", Cpu::CortexA7);
}

// Thumb ADDS Rd, Rd, #imm should prefer format-3 (ADDS Rd, #imm8) when Rd==Rn
#[test]
fn thumb_adds_rd_rd_imm_format3() {
    check_thumb("adds r1, r1, #4", Cpu::CortexM4);
    check_thumb("adds r0, r0, #100", Cpu::CortexM4);
    check_thumb("adds r7, r7, #255", Cpu::CortexM4);
    // Different Rd/Rn should still use format-2
    check_thumb("adds r0, r1, #3", Cpu::CortexM4);
}

// Thumb SUBS Rd, Rd, #imm should prefer format-3 when Rd==Rn
#[test]
fn thumb_subs_rd_rd_imm_format3() {
    check_thumb("subs r1, r1, #4", Cpu::CortexM4);
    check_thumb("subs r2, r2, #100", Cpu::CortexM4);
    check_thumb("subs r0, r1, #3", Cpu::CortexM4);
}

// Thumb MOV without S (outside IT block): must use wide encoding to avoid setting flags
#[test]
fn thumb_mov_no_flags_wide() {
    check_thumb("mov r0, #4", Cpu::CortexM4);
    check_thumb("mov r0, #0", Cpu::CortexM4);
    check_thumb("mov r1, #255", Cpu::CortexM4);
    // MOVS should still be narrow
    check_thumb("movs r0, #4", Cpu::CortexM4);
    check_thumb("movs r1, #255", Cpu::CortexM4);
}

// Thumb MOV inside IT block: narrow is OK (IT suppresses flag setting)
#[test]
fn thumb_mov_in_it_block() {
    check_thumb("cmp r0, #0\nit eq\nmoveq r1, #1", Cpu::CortexM4);
    check_thumb("cmp r0, #1\nit ne\nmovne r1, #0", Cpu::CortexM4);
}

// Thumb DBG instruction: always 32-bit
#[test]
fn thumb_dbg() {
    check_thumb("dbg #0", Cpu::CortexM4);
    check_thumb("dbg #5", Cpu::CortexM4);
}

// Thumb RRX instruction: always 32-bit
#[test]
fn thumb_rrx() {
    check_thumb("rrx r0, r1", Cpu::CortexM4);
    check_thumb("rrx r8, r9", Cpu::CortexM4);
}

// DBG/RRX size prediction affects subsequent label offsets
#[test]
fn thumb_dbg_then_branch() {
    check_thumb("dbg #0\nb target\nnop\ntarget:\nnop", Cpu::CortexM4);
}

#[test]
fn thumb_rrx_then_branch() {
    check_thumb("rrx r0, r1\nb target\nnop\ntarget:\nnop", Cpu::CortexM4);
}

// Thumb CBZ/CBNZ
#[test]
fn thumb_cbz_cbnz() {
    check_thumb("cbz r0, target\nnop\ntarget:\nnop", Cpu::CortexM4);
    check_thumb("cbnz r1, target\nnop\nnop\ntarget:\nnop", Cpu::CortexM4);
}

// Thumb CPSIE/CPSID with flag identifiers
#[test]
fn thumb_cpsie_cpsid() {
    check_thumb("cpsie i", Cpu::CortexM4);
    check_thumb("cpsie f", Cpu::CortexM4);
    check_thumb("cpsid i", Cpu::CortexM4);
    check_thumb("cpsid f", Cpu::CortexM4);
}

// Thumb LDMIA SP! → narrow POP encoding
#[test]
fn thumb_ldmia_sp_as_pop() {
    check_thumb("ldmia sp!, {r0, r3}", Cpu::CortexM4);
    check_thumb("ldmia sp!, {r0-r7}", Cpu::CortexM4);
    check_thumb("ldmia sp!, {r4-r11, pc}", Cpu::CortexM4);
}

// Thumb ADD/SUB SP large immediate (> 508) needs wide encoding
#[test]
fn thumb_add_sub_sp_large_imm() {
    check_thumb("add sp, sp, #512", Cpu::CortexM4);
    check_thumb("sub sp, sp, #512", Cpu::CortexM4);
    // Narrow range still works
    check_thumb("add sp, sp, #4", Cpu::CortexM4);
    check_thumb("sub sp, sp, #508", Cpu::CortexM4);
}

// Multi-instruction sequences that exercise label offset accuracy
#[test]
fn thumb_sha_style_loop() {
    check_thumb(
        "sha_loop:\n\
         eors r0, r4\n\
         eors r1, r5\n\
         eors r2, r6\n\
         eors r3, r7\n\
         cmp r0, r1\n\
         bne sha_loop",
        Cpu::CortexM4,
    );
}

#[test]
fn thumb_context_switch_pattern() {
    check_thumb(
        "mrs r0, psp\n\
         isb\n\
         stmdb r0!, {r4-r11, r14}\n\
         str r0, [r2]\n\
         stmdb sp!, {r0, r3}\n\
         mov r0, #4\n\
         msr basepri, r0\n\
         dsb\n\
         isb\n\
         mov r0, #0\n\
         msr basepri, r0\n\
         ldmia sp!, {r0, r3}\n\
         ldr r1, [r3]\n\
         ldr r0, [r1]\n\
         ldmia r0!, {r4-r11, r14}\n\
         msr psp, r0\n\
         isb\n\
         bx r14",
        Cpu::CortexM4,
    );
}

#[test]
fn a32_sha_compress_pattern() {
    check_a32(
        "sha_loop:\n\
         mov r11, r6, ror #6\n\
         eor r11, r11, r6, ror #11\n\
         eor r11, r11, r6, ror #25\n\
         add r10, r10, r11\n\
         ldr r11, [r0], #4\n\
         add r10, r10, r11\n\
         and r11, r6, r7\n\
         bic r1, r8, r6\n\
         eor r11, r11, r1\n\
         add r10, r10, r11\n\
         cmp r0, r12\n\
         blt sha_loop",
        Cpu::CortexA7,
    );
}

#[test]
fn a32_context_switch_pattern() {
    check_a32(
        "push {r1}\n\
         ldr r0, [r0, #4]\n\
         ldr r1, [r0]\n\
         str sp, [r1]\n\
         ldr r0, [r0]\n\
         ldr r1, [r0]\n\
         ldr sp, [r1]\n\
         pop {r1}\n\
         str r1, [r0]\n\
         pop {r0-r12, r14}",
        Cpu::CortexA7,
    );
}

// ---------------------------------------------------------------------------
// New A32 instructions
// ---------------------------------------------------------------------------

#[test]
fn a32_umaal() {
    check_a32("umaal r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("umaal r4, r5, r6, r7", Cpu::CortexA7);
}

#[test]
fn a32_smulwy() {
    check_a32("smulwb r0, r1, r2", Cpu::CortexA7);
    check_a32("smulwt r0, r1, r2", Cpu::CortexA7);
    check_a32("smulwb r3, r4, r5", Cpu::CortexA7);
    check_a32("smulwt r6, r7, r8", Cpu::CortexA7);
}

#[test]
fn a32_smlawy() {
    check_a32("smlawb r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlawt r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlawb r4, r5, r6, r7", Cpu::CortexA7);
    check_a32("smlawt r8, r9, r10, r11", Cpu::CortexA7);
}

#[test]
fn a32_smmul_round() {
    check_a32("smmulr r0, r1, r2", Cpu::CortexA7);
    check_a32("smmlar r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smmlsr r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smuad_exchange() {
    check_a32("smuadx r0, r1, r2", Cpu::CortexA7);
    check_a32("smusdx r0, r1, r2", Cpu::CortexA7);
    check_a32("smladx r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlsdx r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_smlald_exchange() {
    check_a32("smlaldx r0, r1, r2, r3", Cpu::CortexA7);
    check_a32("smlsldx r0, r1, r2, r3", Cpu::CortexA7);
}

#[test]
fn a32_ssat16_usat16() {
    check_a32("ssat16 r0, #1, r1", Cpu::CortexA7);
    check_a32("ssat16 r2, #8, r3", Cpu::CortexA7);
    check_a32("usat16 r0, #0, r1", Cpu::CortexA7);
    check_a32("usat16 r4, #15, r5", Cpu::CortexA7);
}

#[test]
fn a32_yield() {
    check_a32("yield", Cpu::CortexA7);
}

#[test]
fn a32_swp() {
    check_a32("swp r0, r1, [r2]", Cpu::Arm7Tdmi);
    check_a32("swpb r3, r4, [r5]", Cpu::Arm7Tdmi);
}

#[test]
fn a32_ldrexd_strexd() {
    check_a32("ldrexd r0, r1, [r2]", Cpu::CortexA7);
    check_a32("strexd r0, r2, r3, [r4]", Cpu::CortexA7);
}

#[test]
fn a32_pldw() {
    check_a32("pldw [r0]", Cpu::CortexA7);
    check_a32("pldw [r1, #100]", Cpu::CortexA7);
}

#[test]
fn a32_setend() {
    check_a32("setend be", Cpu::CortexA7);
    check_a32("setend le", Cpu::CortexA7);
}

#[test]
fn balign() {
    // .balign takes a byte count, not a power of two
    check_a32(".byte 0xff\n.balign 4\nmov r0, r0", Cpu::CortexA7);
    check_thumb(".byte 0xff\n.balign 2\nmovs r0, #0", Cpu::CortexM4);
    check_a32(".balign 4\nmov r0, r0", Cpu::CortexA7);
    check_a32(".byte 0xff\n.balign 4, 0xab\nmov r0, r0", Cpu::CortexA7);
}

// ---------------------------------------------------------------------------
// Expression tests
// ---------------------------------------------------------------------------

#[test]
fn expr_label_plus_const() {
    // ADR with label + offset (used for Thumb interworking: set bit[0])
    check_a32("adr r0, 1f + 1\n1: nop", Cpu::CortexA7);
    check_a32("adr r0, 1f + 4\n1: nop", Cpu::CortexA7);
}

#[test]
fn expr_label_minus_const() {
    check_a32("nop\n1: adr r0, 1b - 4", Cpu::CortexA7);
}

#[test]
fn expr_label_diff_in_byte() {
    // .byte with label difference (used for TBB branch tables)
    check_a32(
        "1:\nnop\n2:\n.byte (2b - 1b)\n.balign 4",
        Cpu::CortexA7,
    );
}

#[test]
fn expr_label_diff_div() {
    // (label - label) / 2 — typical TBB table entry
    check_a32(
        "1:\nnop\nnop\n2:\n.byte (2b - 1b) / 2\n.balign 4",
        Cpu::CortexA7,
    );
}

#[test]
fn expr_word_with_label() {
    // .word with a label reference
    check_a32(
        "1: nop\n.word 1b",
        Cpu::CortexA7,
    );
}

#[test]
fn expr_word_arithmetic() {
    // .word with arithmetic on labels
    check_a32(
        "1: nop\n2: nop\n.word 2b - 1b",
        Cpu::CortexA7,
    );
}

#[test]
fn expr_mul() {
    check_a32(".word 3 * 4", Cpu::CortexA7);
    check_a32(".byte 5 * 2", Cpu::CortexA7);
}

#[test]
fn expr_complex() {
    // Parenthesized expressions
    check_a32(".word (2 + 3) * 4", Cpu::CortexA7);
    check_a32(".byte (10 - 2) / 4", Cpu::CortexA7);
}

#[test]
fn expr_branch_label_offset() {
    // B to label + offset
    check_a32("b 1f + 4\nnop\n1: nop\nnop", Cpu::CortexA7);
}

// ---------------------------------------------------------------------------
// .fpu directive
// ---------------------------------------------------------------------------

#[test]
fn fpu_directive() {
    // .fpu directive should be accepted and not affect instruction encoding
    check_a32(".fpu vfpv3-d16\nnop", Cpu::CortexA7);
    check_a32(".fpu vfpv4-d16\nmov r0, #1", Cpu::CortexA7);
}

// ---------------------------------------------------------------------------
// Negative register offset in memory operands
// ---------------------------------------------------------------------------

#[test]
fn ldr_negative_register_offset() {
    // LDR with -Rm (subtract register)
    check_a32("ldr r0, [r1, -r2]", Cpu::CortexA7);
    check_a32("ldr r0, [r1, -r2, lsl #2]", Cpu::CortexA7);
    check_a32("str r0, [r1, -r2]", Cpu::CortexA7);
    check_a32("ldrb r0, [r1, -r2]", Cpu::CortexA7);
    check_a32("ldrh r0, [r1, -r2]", Cpu::CortexA7);
}

#[test]
fn ldr_negative_register_pre_indexed() {
    check_a32("ldr r0, [r1, -r2]!", Cpu::CortexA7);
    check_a32("ldr r0, [r1, -r2, lsl #2]!", Cpu::CortexA7);
}

#[test]
fn ldr_negative_register_post_indexed() {
    check_a32("ldr r0, [r1], -r2", Cpu::CortexA7);
    check_a32("ldr r0, [r1], -r2, lsl #2", Cpu::CortexA7);
}

// ---------------------------------------------------------------------------
// .thumb / .arm mid-stream ISA switching
// ---------------------------------------------------------------------------

#[test]
fn isa_switch_arm_to_thumb_and_back() {
    // Start in ARM, switch to Thumb, switch back to ARM
    check_a32(
        "nop\n.thumb\n.balign 2\nnop\n.arm\n.balign 4\nnop",
        Cpu::CortexA7,
    );
}

#[test]
fn isa_switch_thumb_to_arm() {
    // Two thumb nops (4 bytes) then switch to ARM for clean alignment
    check_thumb(
        "nop\nnop\n.arm\nnop\n.thumb\nnop\nnop",
        Cpu::CortexA7,
    );
}
