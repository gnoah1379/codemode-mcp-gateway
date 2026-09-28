#!/bin/sh
set -eu

repo=${CODE_MODE_GATEWAY_REPO:-gnoah1379/codemode-mcp-gateway}
update=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --repo)
      [ "$#" -ge 2 ] || { echo '--repo needs owner/repo' >&2; exit 2; }
      repo=$2
      shift 2
      ;;
    --update)
      update=1
      shift
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

case "$repo" in
  *[!A-Za-z0-9_./-]*|*/*/*|/*|*/|*/.|*/..|.*/*|../*|*//*|'')
    echo 'repository must be owner/repo' >&2; exit 2 ;;
  */*) ;;
  *) echo 'repository must be owner/repo' >&2; exit 2 ;;
esac

os=$(uname -s)
arch=$(uname -m)
case "$os:$arch" in
  Darwin:arm64) platform=darwin-arm64 ;;
  Darwin:x86_64) platform=darwin-x86_64 ;;
  Linux:x86_64) platform=linux-x86_64 ;;
  *) echo "No prebuilt release for $os/$arch" >&2; exit 1 ;;
esac

asset="code-mode-mcp-server-$platform.tar.gz"
base=${CODE_MODE_GATEWAY_RELEASE_BASE:-"https://github.com/$repo/releases/latest/download"}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
curl -fsSL --retry 3 "$base/$asset" -o "$tmp/$asset"
curl -fsSL --retry 3 "$base/$asset.sha256" -o "$tmp/$asset.sha256"
(
  cd "$tmp"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 -c "$asset.sha256"
  else
    sha256sum -c "$asset.sha256"
  fi
  tar -xzf "$asset" codemode
)

# `curl ... | sh` feeds the script through stdin, so first-run prompts read from the terminal instead.
interactive() {
  if [ -t 0 ]; then
    "$@"
  elif (: </dev/tty) 2>/dev/null; then
    "$@" </dev/tty
  else
    "$@"
  fi
}

bin_dir=${CODE_MODE_GATEWAY_BIN_DIR:-"$HOME/.local/bin"}
mkdir -p "$bin_dir"
bin="$bin_dir/codemode"
staged="$bin_dir/.codemode.$$"
cp "$tmp/codemode" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$bin"
legacy_bin="$bin_dir/code-mode-mcp-server"
if [ -e "$legacy_bin" ] || [ -L "$legacy_bin" ]; then
  rm -f "$legacy_bin"
  ln -s codemode "$legacy_bin"
fi

config_home=${XDG_CONFIG_HOME:-"$HOME/.config"}
config_dir=$config_home/code-mode-gateway
mkdir -p "$config_dir"
printf '%s\n' "$repo" > "$config_dir/release-repo"

if [ "$update" -eq 1 ]; then
  if [ -f "$config_dir/config.yaml" ]; then
    interactive "$bin" init
  fi
  case "$os" in
    Darwin) service_file="$HOME/Library/LaunchAgents/com.code-mode-mcp.gateway.plist" ;;
    Linux) service_file="$config_home/systemd/user/com.code-mode-mcp.gateway.service" ;;
  esac
  if [ -f "$service_file" ]; then
    was_running=0
    if "$bin" service status >/dev/null 2>&1; then was_running=1; fi
    "$bin" service install
    if [ "$was_running" -eq 0 ]; then "$bin" service stop; fi
    echo "Updated $bin and refreshed the service"
  else
    echo "Updated $bin"
  fi
else
  interactive "$bin" init
  "$bin" service install
  echo "Installed $bin and started the background service"
  case ":$PATH:" in
    *":$bin_dir:"*) ;;
    *)
      echo
      echo "$bin_dir is not on your PATH. Add this line to your shell profile (~/.zshrc or ~/.bashrc):"
      echo "  export PATH=\"$bin_dir:\$PATH\""
      ;;
  esac
  echo
  echo "Manage the gateway at http://127.0.0.1:8080 or run: codemode --help"
fi
