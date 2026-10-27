// Node-level regression for the shell signature check. It lives in JS because the
// project does not install @types/node and the code under test is a shell script.
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { expect, test } from 'vitest';

const checkScript = join(process.cwd(), 'scripts/verify-macos-signature.sh');

// Stub codesign: prints the given lines, then far more output than a pipe buffer.
// A reader that quits on the first match closes the pipe while the writer still
// has data, so the writer dies with SIGPIPE - the packaging abort under test.
const codesignStub = (lines, exitCode = 0) => `#!/bin/bash
printf '%s\\n' ${lines.map((line) => `'${line}'`).join(' ')}
head -c 262144 /dev/zero | tr '\\0' 'x'
printf '\\n'
exit ${exitCode}
`;

const runSignatureCheck = (stub) => {
  const dir = mkdtempSync(join(tmpdir(), 'live-translator-signature-'));
  try {
    const codesign = join(dir, 'codesign');
    writeFileSync(codesign, stub);
    chmodSync(codesign, 0o755);
    return spawnSync('/bin/bash', [checkScript, join(dir, 'Live Translator.app'), codesign], { encoding: 'utf8' });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
};

test('signature check accepts an ad-hoc signature when codesign writes past the first match', () => {
  const result = runSignatureCheck(codesignStub(['Executable=/tmp/app', 'Identifier=adhoc', 'Signature=adhoc']));
  expect(result.status).toBe(0);
});

test('signature check rejects a non-ad-hoc signature', () => {
  const result = runSignatureCheck(codesignStub(['Executable=/tmp/app', 'Signature=1234abcd']));
  expect(result.status).not.toBe(0);
  expect(result.stderr).toMatch(/ad-hoc/);
});

test('signature check rejects a failed codesign that still printed an ad-hoc signature', () => {
  const result = runSignatureCheck(codesignStub(['Signature=adhoc'], 3));
  expect(result.status).not.toBe(0);
  expect(result.stderr).toMatch(/codesign --display failed/);
});
