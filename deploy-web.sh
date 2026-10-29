#!/usr/bin/env bash
set -euo pipefail

fail() { printf '部署失败：%s\n' "$*" >&2; return 1; }

usage() {
  cat <<'EOF'
用法：bash deploy-web.sh [--help]
支持 Ubuntu 22.04 / x86_64 / systemd。
请以拥有目标 CLI 配置的普通用户运行，需要 sudo 权限。
首次自动安装构建依赖并编译，后续执行会重新构建和部署。
默认地址 127.0.0.1:8080，数据和令牌保存在 ~/.cc-switch-web/。
EOF
}

check_platform() {
  [[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] ||
    { fail '仅支持 Ubuntu 22.04 / x86_64。'; return 1; }
  # shellcheck source=/dev/null
  source /etc/os-release
  [[ ${ID:-} == ubuntu && ${VERSION_ID:-} == 22.04 ]] ||
    { fail '仅支持 Ubuntu 22.04；其他发行版尚未验证。'; return 1; }
  [[ $(id -u) != 0 ]] ||
    { fail '请以拥有 CLI 配置的普通用户运行，不要使用 sudo bash。'; return 1; }
  [[ -d /run/systemd/system ]] ||
    { fail '需要以 systemd 启动的服务器，不能直接在普通容器中运行。'; return 1; }
}

# EnvironmentFile uses double-quoted values, without shell expansion.
env_quote() {
  local value=$1
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  printf '"%s"' "$value"
}

prepare_credentials() {
  local directory=$1 service_path=$2 token
  mkdir -p "$directory"
  if [[ ! -e $directory/web-access-token ]]; then
    # Noclobber also protects a token created by another process.
    (umask 077; set -o noclobber; openssl rand -hex 32 > "$directory/web-access-token")
  fi
  token=$(cat "$directory/web-access-token")
  token=${token%$'\r'}
  [[ $token =~ ^[[:graph:]]{32,}$ ]] ||
    { fail '已有访问令牌无效，请检查 web-access-token；不会覆盖此文件。'; return 1; }
  [[ $service_path != *$'\n'* && $service_path != *$'\r'* ]] ||
    { fail 'PATH 不能包含换行符。'; return 1; }
  chmod 600 "$directory/web-access-token"
  (
    umask 077
    printf 'CC_SWITCH_WEB_TOKEN=%s\nPATH=%s\n' \
      "$(env_quote "$token")" "$(env_quote "$service_path")" > "$directory/web-service.env"
  )
  chmod 600 "$directory/web-service.env"
}

write_unit() {
  local base=$1 directory=$2 uid=$3 gid=$4
  cat <<EOF
# Managed by cc-switch deploy-web.sh
[Unit]
Description=CC Switch Web management
After=network.target

[Service]
Type=simple
User=$uid
Group=$gid
WorkingDirectory=$base/current
EnvironmentFile=$directory/web-service.env
Environment=CC_SWITCH_WEB_BIND=127.0.0.1:8080
Environment=CC_SWITCH_WEB_ROOT=$base/current/web
ExecStart=$base/current/cc-switch-web
Restart=on-failure
RestartSec=5
TimeoutStopSec=60
UMask=0077

[Install]
WantedBy=multi-user.target
EOF
}

main() {
  if [[ ${1:-} == --help || ${1:-} == -h ]]; then usage; return; fi
  [[ $# == 0 ]] || { usage >&2; fail '未知参数。'; return 1; }
  export LC_ALL=C
  check_platform
  local user_home username uid gid base directory source_root work_dir owner
  username=$(id -un)
  uid=$(id -u)
  gid=$(id -g)
  user_home=$(getent passwd "$uid" | cut -d: -f6)
  [[ $user_home =~ ^/[a-zA-Z0-9._/-]+$ && -d $user_home ]] ||
    { fail '用户主目录必须为不含空格或特殊字符的绝对路径。'; return 1; }
  [[ ${HOME:-} == "$user_home" ]] ||
    { fail 'HOME 与当前用户真实主目录不一致，请使用正常用户登录后部署。'; return 1; }
  base="$user_home/.local/share/cc-switch-web"
  directory="$user_home/.cc-switch-web"
  source_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
  command -v sudo >/dev/null || { fail '缺少 sudo，请先由管理员安装并授权。'; return 1; }
  sudo -v
  if systemctl cat cc-switch-web.service >/dev/null 2>&1; then
    owner=$(systemctl show cc-switch-web.service --property=User --value)
    [[ $owner == "$uid" || $owner == "$username" ]] ||
      { fail '已有服务属于其他用户，停止部署以免接管其配置。'; return 1; }
  fi
  mkdir -p "$base"
  if [[ -e $base/current && ! -L $base/current ]]; then
    fail "$base/current 已存在且不是部署链接，请先检查目录。"
    return 1
  fi
  # A separate lock prevents concurrent deployments, without touching database files.
  exec 9> "$base/deploy.lock"
  flock -n 9 || { fail '已有部署正在进行。'; return 1; }
  work_dir=$(mktemp -d)
  # The path comes directly from mktemp, never from an argument or user data path.
  trap "rm -rf -- '$work_dir'" EXIT

  printf '准备系统依赖（需要 sudo）…\n'
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    ca-certificates curl git openssl xz-utils build-essential pkg-config libssl-dev \
    libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev
  if [[ ! -f $source_root/web-server/Cargo.toml || ! -f $source_root/package.json ]]; then
    source_root="$base/source"
    if [[ -e $source_root ]]; then
      fail "已有源码目录 $source_root；请进入该目录更新代码后执行 bash deploy-web.sh。"
      return 1
    fi
    git clone --depth 1 --branch main https://github.com/sunnyboyi/cc-switch.git "$source_root"
  fi
  cd "$source_root"

  local node_version node_name node_dir pnpm_version pnpm_dir cargo_bin
  node_version=$(tr -d '\r\n' < .node-version)
  [[ $node_version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { fail '.node-version 无效。'; return 1; }
  node_name="node-v$node_version-linux-x64"
  node_dir="$base/toolchain/$node_name"
  if [[ ! -x $node_dir/bin/node ]]; then
    printf '准备仓库指定的 Node.js %s…\n' "$node_version"
    curl --fail --location --retry 3 "https://nodejs.org/dist/v$node_version/$node_name.tar.xz" \
      --output "$work_dir/$node_name.tar.xz"
    curl --fail --location --retry 3 "https://nodejs.org/dist/v$node_version/SHASUMS256.txt" \
      --output "$work_dir/SHASUMS256.txt"
    (cd "$work_dir"; awk -v name="$node_name.tar.xz" '$2 == name' SHASUMS256.txt | sha256sum --check --status)
    mkdir -p "$base/toolchain"
    tar -xJf "$work_dir/$node_name.tar.xz" -C "$base/toolchain"
  fi
  export PATH="$node_dir/bin:$PATH"
  pnpm_version=$(node -p 'JSON.parse(require("fs").readFileSync("package.json", "utf8")).packageManager.replace(/^pnpm@/, "")')
  [[ $pnpm_version =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { fail 'packageManager 必须指定 pnpm 版本。'; return 1; }
  pnpm_dir="$base/toolchain/pnpm-$pnpm_version"
  if [[ ! -x $pnpm_dir/node_modules/.bin/pnpm ]]; then
    npm install --prefix "$pnpm_dir" --no-audit --no-fund "pnpm@$pnpm_version"
  fi
  cargo_bin="${CARGO_HOME:-$user_home/.cargo}/bin"
  export PATH="$pnpm_dir/node_modules/.bin:$cargo_bin:$PATH"
  if ! command -v rustup >/dev/null; then
    curl --proto '=https' --tlsv1.2 --fail --location --retry 3 https://sh.rustup.rs \
      --output "$work_dir/rustup-init.sh"
    sh "$work_dir/rustup-init.sh" -y --no-modify-path --profile minimal --default-toolchain none
  fi
  local rust_version
  rust_version=$(sed -nE 's/^channel *= *"([^"]+)".*/\1/p' rust-toolchain.toml)
  [[ -n $rust_version ]] || { fail 'rust-toolchain.toml 未指定版本。'; return 1; }
  rustup toolchain install "$rust_version" --profile minimal

  printf '构建正式版本（首次编译耗时较长，现有服务继续运行）…\n'
  pnpm install --frozen-lockfile
  pnpm build:web
  mkdir -p dist
  cargo "+$rust_version" build --locked --release --manifest-path web-server/Cargo.toml --target-dir "$base/build"
  local release_dir
  release_dir="$base/releases/$(date +%Y%m%d%H%M%S)-$$"
  mkdir -p "$release_dir"
  install -m 755 "$base/build/release/cc-switch-web" "$release_dir/cc-switch-web"
  cp -R web/dist "$release_dir/web"
  prepare_credentials "$directory" "$PATH"
  write_unit "$base" "$directory" "$uid" "$gid" > "$work_dir/cc-switch-web.service"
  # Validate against the staged executable; current does not exist on first install.
  sed "s|$base/current|$release_dir|g" "$work_dir/cc-switch-web.service" > "$work_dir/cc-switch-web-staged.service"
  systemd-analyze verify "$work_dir/cc-switch-web-staged.service"

  printf '安装并启动后台服务…\n'
  if systemctl cat cc-switch-web.service >/dev/null 2>&1; then sudo systemctl stop cc-switch-web.service; fi
  ln -s "$release_dir" "$base/.current-$$"
  mv -Tf "$base/.current-$$" "$base/current"
  sudo install -m 644 "$work_dir/cc-switch-web.service" /etc/systemd/system/cc-switch-web.service
  sudo systemctl daemon-reload
  sudo systemctl enable cc-switch-web.service
  sudo systemctl restart cc-switch-web.service
  local attempt token
  token=$(cat "$directory/web-access-token")
  token=${token%$'\r'}
  for attempt in {1..60}; do
    if systemctl is-active --quiet cc-switch-web.service &&
      printf 'Authorization: Bearer %s\n' "$token" | \
        curl --fail --silent --max-time 2 --header @- http://127.0.0.1:8080/api/session --output /dev/null; then
      printf '\n部署完成，已配置后台运行和开机自启。\n地址：http://127.0.0.1:8080\n令牌文件：%s/web-access-token\n' "$directory"
      printf '数据目录：%s\n日志：sudo journalctl -u cc-switch-web -n 50 --no-pager\n' "$directory"
      printf '重启：sudo systemctl restart cc-switch-web\n停止：sudo systemctl stop cc-switch-web\n'
      printf '请在本机建立 SSH 隧道：ssh -L 8080:127.0.0.1:8080 %s@服务器地址\n' "$username"
      return
    fi
    sleep 1
  done
  fail '服务未能通过鉴权检查，请运行 sudo journalctl -u cc-switch-web -n 50 --no-pager 查看原因。'
}

if [[ ${BASH_SOURCE[0]} == "$0" ]]; then main "$@"; fi
