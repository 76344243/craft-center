#!/bin/bash
# ArtCraft-Updater: installiert neue Versionen der angepassten Apps aus den Releases der Forks
# (github.com/76344243/<app>/releases), ohne Klicks.
#
# Läuft per LaunchAgent alle 2 Stunden und beim Anmelden. Lädt mit curl (dadurch keine
# Quarantäne-Markierung, also keine Gatekeeper-Abfrage), ersetzt die App in /Applications
# (oder ~/Applications) und meldet sich mit einer Mitteilung. Eine App, die gerade läuft,
# wird nicht angefasst; das Update folgt beim nächsten Durchlauf nach dem Beenden.
#
# Weitere Apps: unten in APPS ergänzen ("repo:AppName").

set -u
OWNER="76344243"
APPS=(
  "wordcraft:WordCraft"
  "pdfcraft:PdfCraft"
  "craft-center:CraftCenter"
)

STATE="$HOME/Library/Application Support/ArtCraft-Updater"
LOG="$HOME/Library/Logs/ArtCraft-Updater.log"
mkdir -p "$STATE" "$(dirname "$LOG")"
exec >>"$LOG" 2>&1

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $*"; }
notify() { /usr/bin/osascript -e "display notification \"$1\" with title \"ArtCraft-Updater\"" >/dev/null 2>&1 || true; }

# Nur ein Durchlauf gleichzeitig.
LOCK="$STATE/lock"
if ! mkdir "$LOCK" 2>/dev/null; then
  # Eine Sperre älter als 2 Stunden stammt von einem abgebrochenen Lauf.
  if [[ -n "$(find "$LOCK" -maxdepth 0 -mmin +120 2>/dev/null)" ]]; then
    rmdir "$LOCK" 2>/dev/null; mkdir "$LOCK" 2>/dev/null || exit 0
  else
    exit 0
  fi
fi
TMP="$(mktemp -d "${TMPDIR:-/tmp}/craft-update.XXXXXX")"
MOUNT=""
cleanup() {
  [[ -n "$MOUNT" ]] && /usr/bin/hdiutil detach -quiet "$MOUNT" >/dev/null 2>&1
  rm -rf "$TMP"
  rmdir "$LOCK" 2>/dev/null
}
trap cleanup EXIT

# Version (tag) und Download-Adresse der ersten .dmg aus der Release-Info, eine pro Zeile.
# JavaScript for Automation ist auf jedem Mac dabei (plutil scheitert an JSON-null).
release_info() {
  /usr/bin/osascript -l JavaScript - "$1" <<'JS' 2>/dev/null
function run(argv) {
  ObjC.import('Foundation');
  const data = $.NSData.dataWithContentsOfFile(argv[0]);
  const o = JSON.parse($.NSString.alloc.initWithDataEncoding(data, $.NSUTF8StringEncoding).js);
  const dmg = (o.assets || []).find(a => a && typeof a.name === 'string' && a.name.endsWith('.dmg'));
  return (o.tag_name || '') + '\n' + (dmg ? dmg.browser_download_url : '');
}
JS
}

update_app() {
  local repo="$1" name="$2"
  local meta="$TMP/$repo.json"
  if ! /usr/bin/curl -fsSL --max-time 60 -H "Accept: application/vnd.github+json" \
      -o "$meta" "https://api.github.com/repos/$OWNER/$repo/releases/latest"; then
    log "$name: Release-Info nicht erreichbar (offline?)"
    return
  fi
  local info tag url
  info="$(release_info "$meta")"
  tag="$(printf '%s\n' "$info" | sed -n 1p)"
  url="$(printf '%s\n' "$info" | sed -n 2p)"
  [[ -z "$tag" ]] && { log "$name: keine Version gefunden"; return; }
  [[ -z "$url" ]] && { log "$name: Release $tag hat keine .dmg"; return; }

  # Wohin: dorthin, wo die App schon liegt; sonst /Applications, falls beschreibbar.
  local dest
  if [[ -d "/Applications/$name.app" ]]; then
    dest="/Applications"
  elif [[ -d "$HOME/Applications/$name.app" ]]; then
    dest="$HOME/Applications"
  elif [[ -w "/Applications" ]]; then
    dest="/Applications"
  else
    dest="$HOME/Applications"
  fi
  mkdir -p "$dest"
  local target="$dest/$name.app"

  local stamp="$STATE/$repo.installed"
  if [[ -d "$target" && -f "$stamp" && "$(cat "$stamp")" == "$tag" ]]; then
    return
  fi

  if /usr/bin/pgrep -f "$name.app/Contents/MacOS/" >/dev/null 2>&1; then
    log "$name: $tag liegt bereit, App läuft gerade, Update beim nächsten Durchlauf"
    return
  fi

  log "$name: installiere $tag"
  local dmg="$TMP/$repo.dmg"
  if ! /usr/bin/curl -fsSL --max-time 900 -o "$dmg" "$url"; then
    log "$name: Download fehlgeschlagen"
    return
  fi
  MOUNT="$TMP/mnt-$repo"
  mkdir -p "$MOUNT"
  if ! /usr/bin/hdiutil attach -nobrowse -readonly -quiet -mountpoint "$MOUNT" "$dmg"; then
    log "$name: .dmg ließ sich nicht öffnen"
    MOUNT=""
    return
  fi
  if [[ ! -d "$MOUNT/$name.app" ]]; then
    log "$name: $name.app nicht in der .dmg"
    /usr/bin/hdiutil detach -quiet "$MOUNT"; MOUNT=""
    return
  fi
  # Erst daneben kopieren, dann austauschen: bricht etwas ab, bleibt die alte App heil.
  local staged="$dest/.$name.app.new"
  rm -rf "$staged"
  if ! /usr/bin/ditto "$MOUNT/$name.app" "$staged"; then
    log "$name: Kopieren nach $dest fehlgeschlagen"
    rm -rf "$staged"
    /usr/bin/hdiutil detach -quiet "$MOUNT"; MOUNT=""
    return
  fi
  /usr/bin/hdiutil detach -quiet "$MOUNT"; MOUNT=""
  /usr/bin/xattr -dr com.apple.quarantine "$staged" 2>/dev/null
  if [[ -d "$target" ]] && ! rm -rf "$target"; then
    log "$name: alte Version in $dest ließ sich nicht ersetzen"
    rm -rf "$staged"
    return
  fi
  mv "$staged" "$target"
  # Finder/Launchpad/Spotlight über die neue Version informieren.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$target" >/dev/null 2>&1
  echo "$tag" >"$stamp"
  log "$name: $tag installiert in $dest"
  notify "$name wurde aktualisiert."
}

for entry in "${APPS[@]}"; do
  update_app "${entry%%:*}" "${entry#*:}"
done
