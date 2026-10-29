import { randomBytes } from "node:crypto";
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { homedir } from "node:os";
import { isIP } from "node:net";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const windows = process.platform === "win32";

export function prepareToken(directory, supplied) {
  const tokenFile = path.join(directory, "web-access-token");
  let token = supplied;
  if (token === undefined) {
    mkdirSync(directory, { recursive: true, mode: 0o700 });
    // Exclusive creation preserves a token created by another launch attempt.
    try {
      writeFileSync(tokenFile, randomBytes(32).toString("hex"), {
        flag: "wx",
        mode: 0o600,
      });
    } catch (error) {
      if (error.code !== "EEXIST") throw error;
    }
    token = readFileSync(tokenFile, "utf8").trim();
  }
  if (!/^[\x21-\x7e]{32,}$/.test(token)) {
    throw new Error(
      "访问令牌至少需要 32 个可打印 ASCII 字符，不能包含空白。请检查 CC_SWITCH_WEB_TOKEN 或令牌文件。",
    );
  }
  return { token, tokenFile: supplied === undefined ? tokenFile : null };
}

export function serverUrl(bind) {
  const match = /^(\[[\da-fA-F:.]+\]|\d+\.\d+\.\d+\.\d+):(\d+)$/.exec(bind);
  if (
    !match ||
    !isIP(match[1].replace(/^\[|\]$/g, "")) ||
    Number(match[2]) < 1 ||
    Number(match[2]) > 65535
  ) {
    throw new Error("CC_SWITCH_WEB_BIND 必须为 IP:端口，例如 127.0.0.1:8080。");
  }
  const host =
    match[1] === "0.0.0.0"
      ? "127.0.0.1"
      : match[1] === "[::]"
        ? "[::1]"
        : match[1];
  return `http://${host}:${match[2]}`;
}

async function run(command, args) {
  // Only fixed package-manager arguments use the Windows command shim.
  const shim = windows && ["pnpm", "corepack"].includes(command);
  const child = spawn(
    shim ? "cmd.exe" : command,
    shim ? ["/d", "/s", "/c", [command, ...args].join(" ")] : args,
    {
      cwd: root,
      stdio: "inherit",
      windowsHide: true,
    },
  );
  await new Promise((resolve, reject) => {
    const failed = () =>
      reject(
        new Error(
          `${command} ${args.join(" ")} 执行失败。请检查上述输出及环境依赖。`,
        ),
      );
    child.once("error", failed);
    child.once("exit", (code) => (code === 0 ? resolve() : failed()));
  });
}

function openBrowser(url) {
  const command = windows
    ? "rundll32.exe"
    : process.platform === "darwin"
      ? "open"
      : "xdg-open";
  const args = windows ? ["url.dll,FileProtocolHandler", url] : [url];
  const browser = spawn(command, args, { stdio: "ignore", windowsHide: true });
  const failed = () => console.log(`浏览器未自动打开，请手动访问 ${url}`);
  browser.on("error", failed);
  browser.on("exit", (code) => {
    if (code) failed();
  });
  browser.unref();
}

async function main() {
  const args = process.argv.slice(2);
  if (args.some((arg) => !["--open", "--no-build"].includes(arg))) {
    throw new Error("用法：pnpm start:web [--open] [--no-build]");
  }
  const bind = process.env.CC_SWITCH_WEB_BIND || "127.0.0.1:8080";
  const url = serverUrl(bind);
  const { token, tokenFile } = prepareToken(
    path.join(process.env.CC_SWITCH_TEST_HOME || homedir(), ".cc-switch-web"),
    process.env.CC_SWITCH_WEB_TOKEN,
  );
  const webRoot = path.resolve(
    root,
    process.env.CC_SWITCH_WEB_ROOT || "web/dist",
  );
  const target = path.resolve(
    root,
    process.env.CARGO_TARGET_DIR || "web-server/target",
  );
  const binary = path.join(
    target,
    "debug",
    `cc-switch-web${windows ? ".exe" : ""}`,
  );
  if (!args.includes("--no-build")) {
    const pnpm = spawnSync(windows ? "pnpm.cmd" : "pnpm", ["--version"], {
      shell: windows,
      windowsHide: true,
    });
    const command = pnpm.status === 0 ? "pnpm" : "corepack";
    const prefix = command === "corepack" ? ["pnpm"] : [];
    console.log("准备依赖与 Web 页面…");
    await run(command, [...prefix, "install", "--frozen-lockfile"]);
    await run(command, [...prefix, "build:web"]);
    console.log("准备服务端（首次编译可能需要几分钟）…");
    await run("cargo", [
      "build",
      "--locked",
      "--manifest-path",
      "web-server/Cargo.toml",
    ]);
  }
  if (!existsSync(binary) || !existsSync(path.join(webRoot, "index.html"))) {
    throw new Error(
      "缺少服务或页面构建产物，请先运行 pnpm start:web 完成构建。",
    );
  }
  const child = spawn(binary, [], {
    cwd: root,
    env: {
      ...process.env,
      CC_SWITCH_WEB_TOKEN: token,
      CC_SWITCH_WEB_BIND: bind,
      CC_SWITCH_WEB_ROOT: webRoot,
    },
    stdio: ["inherit", "pipe", "inherit"],
    windowsHide: true,
  });
  const completion = new Promise((resolve, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) => resolve(signal ? 0 : (code ?? 1)));
  });
  // Windows console broadcasts Ctrl+C to both processes. Killing the child there
  // would bypass the Rust service's normal detach/cleanup path.
  const interrupt = () => {
    if (!windows) child.kill("SIGINT");
  };
  const terminate = () => child.kill("SIGTERM");
  process.on("SIGINT", interrupt);
  process.on("SIGTERM", terminate);
  let output = "";
  let ready = false;
  child.stdout.on("data", (chunk) => {
    process.stdout.write(chunk);
    output = (output + chunk.toString()).slice(-512);
    // Read our own child's readiness message, without sending a credential to
    // whichever unrelated service might already be listening on the port.
    if (!ready && output.includes("CC Switch Web listening on http://")) {
      ready = true;
      console.log(`\n管理页面：${url}`);
      console.log(
        tokenFile
          ? `访问令牌保存在：${tokenFile}\n请打开该文件，将令牌复制到登录框。`
          : "请使用 CC_SWITCH_WEB_TOKEN 中的令牌登录。",
      );
      console.log("按 Ctrl+C 停止服务。\n");
      if (args.includes("--open")) openBrowser(url);
    }
  });
  try {
    process.exitCode = await completion;
  } finally {
    process.off("SIGINT", interrupt);
    process.off("SIGTERM", terminate);
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  main().catch((error) => {
    console.error(`启动失败：${error.message}`);
    process.exitCode = 1;
  });
}
