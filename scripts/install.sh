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
  tar -xzf "$asset" code-mode-mcp-server
)

bin_dir=${CODE_MODE_GATEWAY_BIN_DIR:-"$HOME/.local/bin"}
mkdir -p "$bin_dir"
bin="$bin_dir/code-mode-mcp-server"
staged="$bin_dir/.code-mode-mcp-server.$$"
cp "$tmp/code-mode-mcp-server" "$staged"
chmod 755 "$staged"
mv -f "$staged" "$bin"

config_dir=${XDG_CONFIG_HOME:-"$HOME/.config"}/code-mode-gateway
mkdir -p "$config_dir"
printf '%s\n' "$repo" > "$config_dir/release-repo"

if [ "$update" -eq 1 ]; then
  if "$bin" service status >/dev/null 2>&1; then
    "$bin" service stop
    "$bin" service start
    echo "Updated $bin and restarted the service"
  else
    echo "Updated $bin"
  fi
else
  "$bin" init
  "$bin" service install
  echo "Installed $bin and started the background service"
  echo "Add $bin_dir to PATH if the command is not found in a new shell."
fi
