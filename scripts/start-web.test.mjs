import assert from "node:assert/strict";
import {
  mkdtempSync,
  readFileSync,
  statSync,
  writeFileSync,
  existsSync,
} from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { prepareToken, serverUrl } from "./start-web.mjs";

test("first launch creates a private random token; subsequent launches keep it", () => {
  const directory = path.join(
    mkdtempSync(path.join(tmpdir(), "web-launcher-")),
    ".cc-switch-web",
  );
  const first = prepareToken(directory);
  assert.match(first.token, /^[0-9a-f]{64}$/);
  assert.equal(prepareToken(directory).token, first.token);
  assert.equal(readFileSync(first.tokenFile, "utf8"), first.token);
  if (process.platform !== "win32")
    assert.equal(statSync(first.tokenFile).mode & 0o777, 0o600);
});

test("environment token takes precedence without writing a credential file", () => {
  const directory = path.join(
    mkdtempSync(path.join(tmpdir(), "web-launcher-")),
    "not-created",
  );
  const token = "environment-token-12345678901234567890";
  assert.deepEqual(prepareToken(directory, token), { token, tokenFile: null });
  assert.equal(existsSync(directory), false);
});

test("invalid credentials are rejected and an existing invalid file is never overwritten", () => {
  const directory = mkdtempSync(path.join(tmpdir(), "web-launcher-"));
  const file = path.join(directory, "web-access-token");
  writeFileSync(file, "invalid");
  assert.throws(() => prepareToken(directory), /访问令牌/);
  assert.equal(readFileSync(file, "utf8"), "invalid");
  for (const token of [
    "",
    "short",
    "x".repeat(32) + " ",
    "x".repeat(32) + "\n",
    "中文".repeat(32),
  ]) {
    assert.throws(() => prepareToken(directory, token), /访问令牌/);
  }
});

test("display URLs support IPv4/IPv6 and reject malformed addresses and ports", () => {
  assert.equal(serverUrl("127.0.0.1:8080"), "http://127.0.0.1:8080");
  assert.equal(serverUrl("0.0.0.0:8080"), "http://127.0.0.1:8080");
  assert.equal(serverUrl("[::1]:8080"), "http://[::1]:8080");
  assert.equal(serverUrl("[::]:8080"), "http://[::1]:8080");
  for (const bind of [
    "localhost:8080",
    "127.0.0.1:0",
    "127.0.0.1:65536",
    "999.1.1.1:8080",
    "[:::]:8080",
    "bad-address",
  ]) {
    assert.throws(() => serverUrl(bind), /CC_SWITCH_WEB_BIND/);
  }
});
