/// Optimized Brainfuck interpreter with compilation to internal ops.
/// Equivalent to fastbf2/fastbf3 — supports RLE, `[-]` clear, output buffering,
/// and optional tape tracing at each input instruction.

const TAPE_SIZE: usize = 131_072;

#[derive(Clone, Copy)]
enum Op {
    Add(i32),       // tape[ptr] += arg
    Move(i32),      // ptr += arg
    Output,         // print tape[ptr]
    Input,          // read into tape[ptr]
    JmpZ(usize),    // if tape[ptr]==0, jump to arg
    JmpNZ(usize),   // if tape[ptr]!=0, jump to arg
    Clear,          // tape[ptr] = 0
}

/// Source mapping: compiled op index -> raw BF source byte offset.
pub struct Program {
    ops: Vec<Op>,
    src_map: Vec<usize>,
}

/// Tape snapshot taken at an input instruction.
#[derive(Clone)]
pub struct TapeSnapshot {
    pub input_index: usize,
    pub ptr: usize,
    pub ip: usize,
    pub src_pos: usize,
    pub input_char: u8,
    /// Non-zero cells near the pointer: (offset_from_ptr, value).
    pub nearby: Vec<(i32, u8)>,
    /// Output produced since last snapshot.
    pub output_since: Vec<u8>,
}

pub struct RunResult {
    pub output: Vec<u8>,
    pub tape_snapshots: Vec<TapeSnapshot>,
    pub inputs_consumed: usize,
}

impl Program {
    /// Compile raw BF source into optimized ops.
    pub fn compile(source: &[u8]) -> Self {
        // Strip to BF-only chars, keeping source positions.
        let bf: Vec<(usize, u8)> = source
            .iter()
            .enumerate()
            .filter(|(_, &c)| b"+-<>.,[]".contains(&c))
            .map(|(i, &c)| (i, c))
            .collect();

        let mut ops = Vec::with_capacity(bf.len());
        let mut src_map = Vec::with_capacity(bf.len());
        let mut stack: Vec<usize> = Vec::new();
        let mut i = 0;

        while i < bf.len() {
            let (pos, ch) = bf[i];
            match ch {
                b'+' | b'-' => {
                    let start = pos;
                    let mut val: i32 = 0;
                    while i < bf.len() && (bf[i].1 == b'+' || bf[i].1 == b'-') {
                        val += if bf[i].1 == b'+' { 1 } else { -1 };
                        i += 1;
                    }
                    if val != 0 {
                        src_map.push(start);
                        ops.push(Op::Add(val));
                    }
                }
                b'>' | b'<' => {
                    let start = pos;
                    let mut val: i32 = 0;
                    while i < bf.len() && (bf[i].1 == b'>' || bf[i].1 == b'<') {
                        val += if bf[i].1 == b'>' { 1 } else { -1 };
                        i += 1;
                    }
                    if val != 0 {
                        src_map.push(start);
                        ops.push(Op::Move(val));
                    }
                }
                b'.' => {
                    src_map.push(pos);
                    ops.push(Op::Output);
                    i += 1;
                }
                b',' => {
                    src_map.push(pos);
                    ops.push(Op::Input);
                    i += 1;
                }
                b'[' => {
                    // Detect [-] or [+] clear pattern.
                    if i + 2 < bf.len()
                        && (bf[i + 1].1 == b'-' || bf[i + 1].1 == b'+')
                        && bf[i + 2].1 == b']'
                    {
                        src_map.push(pos);
                        ops.push(Op::Clear);
                        i += 3;
                    } else {
                        src_map.push(pos);
                        ops.push(Op::JmpZ(0)); // patched later
                        stack.push(ops.len() - 1);
                        i += 1;
                    }
                }
                b']' => {
                    let open = stack.pop().expect("unmatched ]");
                    let close = ops.len();
                    src_map.push(pos);
                    ops.push(Op::JmpNZ(open));
                    ops[open] = Op::JmpZ(close);
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        Program { ops, src_map }
    }

    pub fn op_count(&self) -> usize {
        self.ops.len()
    }

    pub fn src_pos(&self, ip: usize) -> usize {
        self.src_map[ip]
    }

    /// Run the program with given input bytes.
    /// If `trace` is true, captures tape snapshots at each input instruction.
    pub fn run(&self, input: &[u8], trace: bool) -> RunResult {
        let mut tape = vec![0u8; TAPE_SIZE];
        let mut ptr: usize = TAPE_SIZE / 2;
        let mut ip: usize = 0;
        let mut input_pos: usize = 0;
        let mut output = Vec::new();
        let mut snapshots = Vec::new();
        let mut output_since_last = Vec::new();
        let nops = self.ops.len();

        while ip < nops {
            match self.ops[ip] {
                Op::Add(v) => {
                    tape[ptr] = tape[ptr].wrapping_add(v as u8);
                }
                Op::Move(v) => {
                    ptr = (ptr as i64 + v as i64) as usize;
                }
                Op::Output => {
                    output.push(tape[ptr]);
                    if trace {
                        output_since_last.push(tape[ptr]);
                    }
                }
                Op::Input => {
                    if input_pos >= input.len() {
                        break; // EOF
                    }
                    if trace {
                        let nearby = self.snapshot_nearby(&tape, ptr);
                        snapshots.push(TapeSnapshot {
                            input_index: input_pos,
                            ptr,
                            ip,
                            src_pos: self.src_map[ip],
                            input_char: input[input_pos],
                            nearby,
                            output_since: std::mem::take(&mut output_since_last),
                        });
                    }
                    tape[ptr] = input[input_pos];
                    input_pos += 1;
                }
                Op::JmpZ(target) => {
                    if tape[ptr] == 0 {
                        ip = target;
                    }
                }
                Op::JmpNZ(target) => {
                    if tape[ptr] != 0 {
                        ip = target;
                    }
                }
                Op::Clear => {
                    tape[ptr] = 0;
                }
            }
            ip += 1;
        }

        RunResult {
            output,
            tape_snapshots: snapshots,
            inputs_consumed: input_pos,
        }
    }

    fn snapshot_nearby(&self, tape: &[u8], ptr: usize) -> Vec<(i32, u8)> {
        let mut result = Vec::new();
        let radius: i32 = 120;
        for offset in -radius..=radius {
            let idx = ptr as i64 + offset as i64;
            if idx >= 0 && (idx as usize) < tape.len() {
                let val = tape[idx as usize];
                if val != 0 {
                    result.push((offset, val));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_world() {
        let src = b"++++++++[>++++[>++>+++>+++>+<<<<-]>+>+>->>+[<]<-]>>.>---.+++++++..+++.>>.<-.<.+++.------.--------.>>+.>++.";
        let prog = Program::compile(src);
        let result = prog.run(b"", false);
        assert_eq!(String::from_utf8_lossy(&result.output), "Hello World!\n");
    }

    #[test]
    fn cat_program() {
        let src = b",[.,]";
        let prog = Program::compile(src);
        let result = prog.run(b"abc", false);
        assert_eq!(&result.output, b"abc");
    }

    #[test]
    fn clear_loop() {
        let src = b"+++++[-]>.";
        let prog = Program::compile(src);
        let result = prog.run(b"", false);
        assert_eq!(result.output, vec![0]);
    }

    #[test]
    fn trace_captures_snapshots() {
        let src = b",.,+.";
        let prog = Program::compile(src);
        let result = prog.run(b"A", true);
        assert_eq!(result.tape_snapshots.len(), 1);
        assert_eq!(result.tape_snapshots[0].input_char, b'A');
        assert_eq!(result.output, vec![b'A', b'A' + 1]);
    }
}
