//! The console's geometry stage (the legacy GS of GFX10, `Gs` programs) as a
//! DXIL geometry shader, a child module of the lowering so it shares its
//! state.
//!
//! On the console the GS runs merged with the ES variant of the pipeline's
//! vertex shader. The ES writes each vertex's outputs into LDS, one 16-byte
//! item per output register (the ESGS ring); the GS finds its triangle's
//! three vertices there by the dword offsets the hardware passes in v0
//! (vertices 0 and 1, 16 bits each) and v1 (vertex 2), writes the vertices
//! it outputs into an LDS area of its own (16 bytes per output register a
//! vertex), and tells the hardware with `s_sendmsg MSG_GS` when one is
//! complete (`GS_OP_EMIT`) and when a strip ends (`GS_OP_CUT`); the driver
//! exports what it wrote.
//!
//! On the host the pipeline's vertex shader is the container's plain `Vs`
//! variant, already translated, whose outputs are what the ES variant writes
//! to the ring (same registers), and the GS is a geometry shader:
//!
//! - the ring is the GS's input array: v0 and v1 place vertex `n` at item
//!   `n`, so an LDS read's base names the vertex (base / item size) and its
//!   immediate offset the register and component, read with `loadInput`;
//! - an LDS write's immediate offset names the output vertex, register and
//!   component (every write of a GS lane goes through one base register:
//!   checked on all of Grand Theft Auto VI's 195 programs), kept until the
//!   vertex is emitted;
//! - `GS_OP_EMIT` stores the next vertex's kept values as outputs and emits
//!   it (`emitStream`), `GS_OP_CUT` ends the strip (`cutStream`);
//! - the prologue that sets EXEC from the wave's lane count is left out: a
//!   host invocation is one console lane, which runs.
//!
//! shortcut: GS lanes' output areas are told apart by nothing but their base
//! register; a program that writes one vertex through two bases, or emits in
//! a loop, is refused (none of the title's does either).

use std::collections::HashMap;

use super::{Attr, Bin, Fields, Inst, Lower, Operand, Reg, V, Why};
use crate::emit::container::SigElement;

/// What the lowering of a geometry program keeps.
pub(crate) struct Gs {
    /// Bytes of one input vertex in the ring: 16 per input register.
    pub(crate) item_bytes: u32,
    /// Bytes of one output vertex: 16 per output register.
    vertex_bytes: u32,
    /// Dwords written for output vertices: (vertex, register, component).
    slots: HashMap<(u32, u32, u32), V>,
    /// Vertices emitted so far.
    emitted: u32,
}

impl Gs {
    pub(crate) fn new(inputs: &[SigElement], outputs: &[SigElement]) -> Gs {
        let registers = |s: &[SigElement]| s.iter().filter_map(|e| e.row).max().map_or(1, |r| r + 1);
        Gs {
            item_bytes: 16 * registers(inputs),
            vertex_bytes: 16 * registers(outputs),
            slots: HashMap::new(),
            emitted: 0,
        }
    }

    /// The values of the input VGPRs v0 and v1: the dword offsets of the
    /// triangle's vertices in the ring, vertex `n` at item `n`.
    pub(crate) fn vertex_offsets(&self) -> (u32, u32) {
        let dwords = self.item_bytes / 4;
        (dwords << 16, 2 * dwords)
    }
}

/// `s_sendmsg` messages and operations (simm16 bits 0-3 and 4-5).
const MSG_GS: u16 = 2;
const MSG_GS_DONE: u16 = 3;
const GS_OP_CUT: u16 = 1;
const GS_OP_EMIT: u16 = 2;

/// The vertices a geometry program emits (its `GS_OP_EMIT` messages), the
/// geometry shader's maximum: `None` if it emits none.
pub(crate) fn emitted_vertices(insts: &[&Inst]) -> Option<u32> {
    let n = insts
        .iter()
        .filter(|i| {
            i.name() == "s_sendmsg"
                && matches!(i.fields, Fields::Sopp { imm } if imm & 0xF == MSG_GS && imm >> 4 & 2 != 0)
        })
        .count() as u32;
    (n > 0).then_some(n)
}

/// The code of a geometry program with its lane-count prologue left out
/// (`s_bfm_b64 exec, s0, 0` and `s_cselect_b64 exec, -1, exec` become
/// `s_nop`): a host invocation is one console lane, and it runs. Its load
/// through the driver's own table (user SGPRs 0-1, no root parameter:
/// `s_load_dword s0, s[0:1], 0xb8`), whose value only places the lane's
/// output area in LDS (the base of its writes, which [`Lower::gs_ds`] does
/// not need), becomes `s_mov_b32 s0, 0`. `None` if the prologue is not the
/// one the title's programs have.
pub(crate) fn without_lane_count(code: &[u8]) -> Option<Vec<u8>> {
    const BFM_EXEC: u32 = 0x92FE_8000;
    const CSELECT_EXEC: u32 = 0x85FE_7EC1;
    const DRIVER_LOAD: [u32; 2] = [0xF400_0000, 0xFA00_00B8];
    const MOV_S0_0: u32 = 0xBE80_0380;
    const NOP: u32 = 0xBF80_0000;
    let word = |at: usize| code.get(at..at + 4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]));
    // In the first instructions, on dword boundaries.
    let find = |want: u32| (0..64).step_by(4).find(|&at| word(at) == Some(want));
    let (bfm, cselect) = (find(BFM_EXEC)?, find(CSELECT_EXEC)?);
    let mut out = code.to_vec();
    for at in [bfm, cselect] {
        out[at..at + 4].copy_from_slice(&NOP.to_le_bytes());
    }
    if let Some(at) = (0..128)
        .step_by(4)
        .find(|&at| word(at) == Some(DRIVER_LOAD[0]) && word(at + 4) == Some(DRIVER_LOAD[1]))
    {
        out[at..at + 4].copy_from_slice(&MOV_S0_0.to_le_bytes());
        out[at + 4..at + 8].copy_from_slice(&NOP.to_le_bytes());
    }
    Some(out)
}

impl<'a> Lower<'a> {
    /// An LDS read or write of a geometry program: an input vertex's
    /// register, or an output vertex's.
    pub(super) fn gs_ds(&mut self, inst: &Inst) -> Result<(), Why> {
        let Fields::Ds {
            dst,
            addr,
            data,
            offset0,
            offset1,
            gds,
        } = &inst.fields
        else {
            unreachable!()
        };
        if *gds {
            return Err("GDS in a geometry program");
        }
        let name = inst.name();
        let (o0, o1) = (u32::from(*offset0), u32::from(*offset1));
        // Byte offsets of the dwords moved.
        let offsets: Vec<u32> = match name {
            "ds_read_b32" | "ds_write_b32" => vec![o1 << 8 | o0],
            "ds_read_b64" | "ds_write_b64" => {
                let at = o1 << 8 | o0;
                vec![at, at + 4]
            }
            "ds_read2_b32" | "ds_write2_b32" => vec![4 * o0, 4 * o1],
            _ => return Err("an LDS operation in a geometry program"),
        };
        let gs = self.gs.as_ref().ok_or("a geometry program's state")?;
        let (item_bytes, vertex_bytes) = (gs.item_bytes, gs.vertex_bytes);
        let c = self.cur;
        if name.starts_with("ds_read") {
            let Some(Operand::Vector { reg, .. }) = *dst else {
                return Err("an LDS read without a destination");
            };
            // The vertex: the base is its item in the ring.
            let base = self.vgprs(inst, addr.as_ref().ok_or("an LDS address")?, 1)?[0];
            let item = self.s.m.i32(item_bytes);
            let vertex = self.s.m.bin(c, Bin::UDiv, base, item);
            for (k, &off) in offsets.iter().enumerate() {
                if off >= item_bytes {
                    return Err("an LDS read past a ring item");
                }
                let (register, component) = (off / 16, off % 16 / 4);
                let found = self.s.inputs.iter().enumerate().find(|(_, e)| {
                    e.row == Some(register)
                        && (u32::from(e.col)..u32::from(e.col + e.cols)).contains(&component)
                });
                let value = match found {
                    Some((id, e)) => {
                        let float = e.comp == 9;
                        let (name, ty) = if float {
                            ("loadInput.f32", self.s.t.f32)
                        } else {
                            ("loadInput.i32", self.s.t.i32)
                        };
                        let args = [
                            self.s.m.i32(id as u32),
                            self.s.m.i32(0),
                            self.s.m.i8((component - u32::from(e.col)) as u8),
                            vertex,
                        ];
                        let x = self.s.op(c, name, Attr::ReadNone, ty, 4, &args);
                        if float { self.as_int(x) } else { x }
                    }
                    // A component the ES does not write.
                    None => self.s.m.i32(0),
                };
                self.write(inst, Reg::V(reg + k as u8), value, None);
            }
            return Ok(());
        }
        for (k, &off) in offsets.iter().enumerate() {
            let op = data.get(if offsets.len() == 2 && name.contains("write2") { k } else { 0 });
            let op = op.ok_or("an LDS write's data")?;
            // `ds_write_b64`: the pair's two dwords from one operand.
            let x = match (name, *op) {
                ("ds_write_b64", Operand::Vector { reg, .. }) => {
                    let r = Operand::Vector { reg: reg + k as u8, dwords: 1 };
                    self.vgprs(inst, &r, 1)?[0]
                }
                _ => self.vgprs(inst, op, 1)?[0],
            };
            let vertex = off / vertex_bytes;
            let within = off % vertex_bytes;
            let gs = self.gs.as_mut().ok_or("a geometry program's state")?;
            gs.slots.insert((vertex, within / 16, within % 16 / 4), x);
        }
        Ok(())
    }

    /// `s_sendmsg` in a geometry program: a vertex emitted, a strip ended.
    pub(super) fn gs_sendmsg(&mut self, inst: &Inst) -> Result<(), Why> {
        let Fields::Sopp { imm } = inst.fields else {
            unreachable!()
        };
        let (msg, op, stream) = (imm & 0xF, imm >> 4 & 3, imm >> 8 & 3);
        if msg == MSG_GS_DONE || msg != MSG_GS {
            return Ok(());
        }
        if stream != 0 {
            return Err("a geometry program's stream other than 0");
        }
        let c = self.cur;
        let zero = self.s.m.i8(0);
        if op & GS_OP_EMIT != 0 {
            let gs = self.gs.as_mut().ok_or("a geometry program's state")?;
            let vertex = gs.emitted;
            gs.emitted += 1;
            let slots: Vec<(usize, u8, Option<V>, bool)> = self
                .s
                .outputs
                .iter()
                .enumerate()
                .flat_map(|(id, e)| {
                    let row = e.row;
                    (0..e.cols).map(move |col| (id, col, row, e.col, e.comp == 9))
                })
                .map(|(id, col, row, first, float)| {
                    let value = row.and_then(|r| {
                        gs.slots
                            .get(&(vertex, r, u32::from(first + col)))
                            .copied()
                    });
                    (id, col, value, float)
                })
                .collect();
            for (id, col, value, float) in slots {
                let value = match value {
                    Some(v) => v,
                    None => self.s.m.i32(0),
                };
                let (name, value) = if float {
                    ("storeOutput.f32", self.as_float(value))
                } else {
                    ("storeOutput.i32", value)
                };
                let args = [self.s.m.i32(id as u32), self.s.m.i32(0), self.s.m.i8(col), value];
                let void = self.s.t.void;
                self.s.op(c, name, Attr::NoUnwind, void, 5, &args);
            }
            let void = self.s.t.void;
            self.s.op(c, "emitStream", Attr::NoUnwind, void, 97, &[zero]);
        }
        if op & GS_OP_CUT != 0 {
            let void = self.s.t.void;
            self.s.op(c, "cutStream", Attr::NoUnwind, void, 98, &[zero]);
        }
        Ok(())
    }
}
