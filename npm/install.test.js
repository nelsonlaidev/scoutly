const assert = require("node:assert/strict");
const test = require("node:test");

const { artifactFor, checksumFor } = require("./install");

test("maps supported Node.js platforms to GoReleaser archives", () => {
  const cases = [
    ["darwin", "arm64", "scoutly_Darwin_arm64.tar.gz"],
    ["darwin", "x64", "scoutly_Darwin_x86_64.tar.gz"],
    ["linux", "arm64", "scoutly_Linux_arm64.tar.gz"],
    ["linux", "ia32", "scoutly_Linux_i386.tar.gz"],
    ["linux", "x64", "scoutly_Linux_x86_64.tar.gz"],
    ["win32", "arm64", "scoutly_Windows_arm64.zip"],
    ["win32", "ia32", "scoutly_Windows_i386.zip"],
    ["win32", "x64", "scoutly_Windows_x86_64.zip"],
  ];

  for (const [platform, arch, artifact] of cases) {
    assert.equal(artifactFor(platform, arch), artifact);
  }
});

test("rejects unsupported platforms", () => {
  assert.throws(() => artifactFor("freebsd", "x64"), /does not support freebsd-x64/);
});

test("finds an artifact checksum", () => {
  const expected = "a".repeat(64);
  const contents = `${"b".repeat(64)}  another.tar.gz\n${expected}  scoutly.tar.gz\n`;
  assert.equal(checksumFor(contents, "scoutly.tar.gz"), expected);
});

test("rejects a missing or invalid checksum", () => {
  assert.throws(() => checksumFor("invalid  scoutly.tar.gz\n", "scoutly.tar.gz"), /No SHA-256/);
  assert.throws(() => checksumFor(`${"a".repeat(64)}  other.tar.gz\n`, "scoutly.tar.gz"), /No SHA-256/);
});
