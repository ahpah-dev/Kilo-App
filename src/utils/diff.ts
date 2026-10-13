export type DiffLineType = 'ctx' | 'add' | 'del' | 'gap';

export interface DiffLine {
  type: DiffLineType;
  text: string;
}

const MAX_CELLS = 400_000;
const CONTEXT = 3;

function splitLines(text: string): string[] {
  const lines = text.replace(/\r\n/g, '\n').split('\n');
  if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
  return lines;
}

/** Line diff via LCS, collapsing long unchanged runs to a few lines of context. */
export function lineDiff(before: string, after: string): DiffLine[] {
  const a = splitLines(before);
  const b = splitLines(after);

  if (a.length * b.length > MAX_CELLS) {
    return [...a.map((text) => ({ type: 'del' as const, text })), ...b.map((text) => ({ type: 'add' as const, text }))];
  }

  const n = a.length;
  const m = b.length;
  const dp: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      dp[i][j] = a[i] === b[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }

  const full: DiffLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      full.push({ type: 'ctx', text: a[i] });
      i++;
      j++;
    } else if (dp[i + 1][j] >= dp[i][j + 1]) {
      full.push({ type: 'del', text: a[i++] });
    } else {
      full.push({ type: 'add', text: b[j++] });
    }
  }
  while (i < n) full.push({ type: 'del', text: a[i++] });
  while (j < m) full.push({ type: 'add', text: b[j++] });

  const keep = full.map(() => false);
  full.forEach((line, idx) => {
    if (line.type === 'ctx') return;
    for (let k = Math.max(0, idx - CONTEXT); k <= Math.min(full.length - 1, idx + CONTEXT); k++) keep[k] = true;
  });

  const out: DiffLine[] = [];
  let skipped = 0;
  full.forEach((line, idx) => {
    if (keep[idx]) {
      if (skipped) out.push({ type: 'gap', text: `${skipped} unchanged line${skipped === 1 ? '' : 's'}` });
      skipped = 0;
      out.push(line);
    } else {
      skipped++;
    }
  });
  if (skipped) out.push({ type: 'gap', text: `${skipped} unchanged line${skipped === 1 ? '' : 's'}` });
  return out;
}

export function diffStats(lines: DiffLine[]): { added: number; removed: number } {
  let added = 0;
  let removed = 0;
  for (const l of lines) {
    if (l.type === 'add') added++;
    else if (l.type === 'del') removed++;
  }
  return { added, removed };
}
