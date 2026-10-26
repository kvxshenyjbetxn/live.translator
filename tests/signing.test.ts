import { expect, test } from 'vitest';
import pkg from '../package.json';
import lock from '../package-lock.json';
import config from '../src-tauri/tauri.conf.json';
import workspace from '../Cargo.toml?raw';
import cargoLock from '../Cargo.lock?raw';
import desktopManifest from '../src-tauri/Cargo.toml?raw';
import coreManifest from '../crates/translator-core/Cargo.toml?raw';

test('macOS bundles explicitly use free ad-hoc signing', () => {
  expect(config.bundle.macOS.signingIdentity).toBe('-');
});

test('first-party release versions stay synchronized', () => {
  const version = pkg.version;
  expect(lock.version).toBe(version);
  expect(lock.packages[''].version).toBe(version);
  expect(config.version).toBe(version);
  expect(workspace).toContain(`version = "${version}"`);
  for (const name of ['live-translator', 'translator-core']) {
    expect(cargoLock).toContain(`name = "${name}"\nversion = "${version}"`);
  }
  for (const manifest of [desktopManifest, coreManifest]) {
    expect(manifest).toMatch(/version\.workspace\s*=\s*true/);
  }
});
