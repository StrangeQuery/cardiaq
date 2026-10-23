// The browser side of the wasm tests: runs a session script (see tools/session.rs) on heartburn.wasm through its C
// API and returns what happened, line for line as tools/session.rs prints it.
function runSession(wasm, script) {
  const x = wasm.exports;
  const mem = () => x.memory.buffer;
  const put = (words) => { const p = x.hb_buffer(words.length); new Int32Array(mem(), p, words.length).set(words); return words.length; };
  const ok = (r, what) => { if (r !== 0) throw new Error(what + ' refused'); };
  x.hb_reset();
  let input = [], events = [], punched = [], budget = 1000000, runs = 100;
  const take = () => {
    const n = x.hb_punched_len();
    punched.push(...new Int32Array(mem(), x.hb_punched(), n));
    x.hb_take_punched();
  };
  for (const line of script.split('\n')) {
    const w = line.trim().split(/\s+/).filter(Boolean);
    if (!w.length) continue;
    const n = w.slice(1).map(Number);
    switch (w[0]) {
      case 'tape': ok(x.hb_add_tape(put(n)), 'tape'); break;
      case 'cells': ok(x.hb_load_cells(put(n)), 'cells'); break;
      case 'pc': ok(x.hb_set_pc(n[0]), 'pc'); break;
      case 'acc': ok(x.hb_set_acc(n[0]), 'acc'); break;
      case 'push': ok(x.hb_push(put(n)), 'push'); break;
      case 'input': input.push(...n); break;
      case 'break': ok(x.hb_set_break(n[0]), 'break'); break;
      case 'step': for (let i = 0; i < n[0]; i++) { const e = x.hb_step(); take(); events.push(`s${e}@${x.hb_pc()}`); } break;
      case 'budget': budget = n[0]; break;
      case 'runs': runs = n[0]; break;
      default: throw new Error('not a script line: ' + line);
    }
  }
  for (let i = 0; i < runs; i++) {
    const e = x.hb_run(budget);
    take();
    events.push(`${e}@${x.hb_pc()}`);
    if (e === 1) break;
    if (e === 3) { if (!input.length) break; ok(x.hb_push(put([input.shift()])), 'push'); }
  }
  const cells = Array.from(new Int32Array(mem(), x.hb_memory(), 100));
  const owners = Array.from(new Uint8Array(mem(), x.hb_owners(), 100));
  const written = Array.from(new BigUint64Array(mem(), x.hb_written_at(), 100), Number);
  const trail = Array.from(new Int32Array(mem(), x.hb_trail(), 3));
  const nr = x.hb_reader();
  const reader = Array.from(new Int32Array(mem(), x.hb_buffer_ptr(), nr));
  if (x.hb_reader_len() !== nr) throw new Error('hb_reader_len disagrees with hb_reader');
  const ran = Array.from(new Uint8Array(mem(), x.hb_ran(), 100)), used = Array.from(new Uint8Array(mem(), x.hb_used(), 100));
  return [
    `events: ${events.join(' ')}`, `punched: ${punched.join(' ')}`, `steps: ${x.hb_steps()}`, `acc: ${x.hb_acc()}`,
    `pc: ${x.hb_pc()}`, `phase: ${x.hb_phase()}`, `memory: ${cells.join(' ')}`, `owners: ${owners.join(' ')}`,
    `written: ${written.join(' ')}`, `trail: ${trail.join(' ')}`, `reader: ${reader.join(' ')}`,
    `ran: ${ran.join(' ')}`, `used: ${used.join(' ')}`, `last: ${x.hb_last_cell()} ${x.hb_last_word()} ${x.hb_last_acc()}`,
  ].join('\n') + '\n';
}
