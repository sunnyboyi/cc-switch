import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(
  new URL("../deploy-web.sh", import.meta.url),
).replace(/\\/g, "/");
const bash =
  process.platform === "win32" &&
  existsSync("C:/Program Files/Git/bin/bash.exe")
    ? "C:/Program Files/Git/bin/bash.exe"
    : "bash";

function shell(code) {
  return spawnSync(bash, ["-c", code, "deploy-web-test", script], {
    encoding: "utf8",
  });
}

test("deployment help works without installing anything; unknown arguments fail", () => {
  const help = shell('bash "$1" --help');
  assert.equal(help.status, 0, help.stderr);
  assert.match(help.stdout, /Ubuntu 22.04/);
  assert.match(help.stdout, /Alibaba Cloud Linux 3/);
  const invalid = shell('bash "$1" --unknown');
  assert.notEqual(invalid.status, 0);
  assert.match(invalid.stderr, /未知参数/);
});

test("only supported distribution versions are accepted", () => {
  const result = shell(`
    source "$1"
    check_distribution ubuntu 22.04
    check_distribution alinux 3
    check_distribution alinux 3.2104
    for pair in 'ubuntu 24.04' 'alinux 2' 'alinux 4' 'alinux 30' 'centos 8'; do
      if check_distribution $pair; then exit 1; fi
    done
  `);
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

for (const distribution of ["ubuntu", "alinux"]) {
  test(`${distribution} installs native build dependencies without desktop libraries`, () => {
    const result = shell(`
      source "$1"
      sudo() { printf '%s\\n' "$*"; }
      install_dependencies ${distribution}
    `);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.match(
      result.stdout,
      distribution === "ubuntu" ? /apt-get install/ : /dnf install/,
    );
    assert.match(
      result.stdout,
      distribution === "ubuntu" ? /libssl-dev/ : /openssl-devel/,
    );
    assert.doesNotMatch(result.stdout, /gtk|webkit|soup|appindicator|xvfb/i);
  });
}

test("repeated deployments preserve the token and quote service credentials without shell expansion", () => {
  const result = shell(String.raw`
    source "$1"
    export LC_ALL=C
    fixture=$(mktemp -d)
    trap 'rm -rf -- "$fixture"' EXIT
    prepare_credentials "$fixture/data" '/usr/local/bin:/usr/bin'
    first=$(cat "$fixture/data/web-access-token")
    [[ $first =~ ^[a-f0-9]{64}$ ]]
    prepare_credentials "$fixture/data" '/usr/bin'
    [[ $(cat "$fixture/data/web-access-token") == "$first" ]]
    grep -Fx 'PATH="/usr/bin"' "$fixture/data/web-service.env"
    printf '%s' 'Abcdefghijklmnopqrstuvwxyz1234567890"\$value' > "$fixture/data/web-access-token"
    prepare_credentials "$fixture/data" '/usr/bin'
    cat "$fixture/data/web-service.env"
    if [[ $(uname -s) == Linux ]]; then
      [[ $(stat -c %a "$fixture/data/web-access-token") == 600 ]]
      [[ $(stat -c %a "$fixture/data/web-service.env") == 600 ]]
    fi
  `);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.ok(result.stdout.includes(String.raw`1234567890\"\\$value`));
});

test("an invalid existing token aborts without overwriting credentials or data", () => {
  const result = shell(`
    source "$1"
    export LC_ALL=C
    fixture=$(mktemp -d)
    trap 'rm -rf -- "$fixture"' EXIT
    printf invalid > "$fixture/web-access-token"
    printf keep-existing-config > "$fixture/web-service.env"
    printf database-sentinel > "$fixture/cc-switch-web.db"
    if prepare_credentials "$fixture" '/usr/bin'; then exit 1; fi
    [[ $(cat "$fixture/web-access-token") == invalid ]]
    [[ $(cat "$fixture/web-service.env") == keep-existing-config ]]
    [[ $(cat "$fixture/cc-switch-web.db") == database-sentinel ]]
  `);
  assert.equal(result.status, 0, result.stdout + result.stderr);
});

test("unsupported hosts stop before requesting privileges or installing packages", () => {
  const result = shell(`
    source "$1"
    uname() { printf 'unsupported\\n'; }
    sudo() { echo unexpected-privilege-request; exit 99; }
    if check_platform; then exit 1; fi
  `);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.doesNotMatch(result.stdout, /unexpected-privilege-request/);
});

test(
  "generated systemd unit is accepted with private credentials and a staged executable",
  { skip: process.platform !== "linux" },
  () => {
    const result = shell(`
      source "$1"
      fixture=$(mktemp -d)
      trap 'rm -rf -- "$fixture"' EXIT
      mkdir -p "$fixture/install/current/web"
      cp /usr/bin/true "$fixture/install/current/cc-switch-web"
      prepare_credentials "$fixture/data" '/usr/bin:/bin'
      write_unit "$fixture/install" "$fixture/data" "$(id -u)" "$(id -g)" > "$fixture/cc-switch-web.service"
      systemd-analyze verify "$fixture/cc-switch-web.service"
    `);
    assert.equal(result.status, 0, result.stdout + result.stderr);
  },
);
