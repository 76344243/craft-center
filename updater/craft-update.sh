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

# Standard-Apps: Dateityp (UTI) = Kennung der App. Acrobat und Office holen sich diese Typen
# gern still zurück; jeder Durchlauf stellt sie wieder auf die Craft-Apps (nur wenn installiert).
DEFAULTS=(
  "com.adobe.pdf=ai.storyteller.pdfcraft=PdfCraft"
  "org.openxmlformats.wordprocessingml.document=ai.storyteller.wordcraft=WordCraft"
)

STATE="$HOME/Library/Application Support/ArtCraft-Updater"
LOG="$HOME/Library/Logs/ArtCraft-Updater.log"
mkdir -p "$STATE" "$(dirname "$LOG")"
exec >>"$LOG" 2>&1

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') $*"; }
notify() { /usr/bin/osascript -e "display notification \"$1\" with title \"ArtCraft-Updater\"" >/dev/null 2>&1 || true; }

# Nur ein Durchlauf gleichzeitig. Die Sperre nennt den Prozess, der sie hält; gehört sie einem
# Prozess, der nicht mehr läuft (abgebrochen, beim Neueinrichten beendet), wird sie übernommen.
LOCK="$STATE/lock"
take_lock() {
  if mkdir "$LOCK" 2>/dev/null; then
    echo $$ >"$LOCK/pid"
    return 0
  fi
  local holder
  holder="$(cat "$LOCK/pid" 2>/dev/null)"
  if [[ -n "$holder" ]] && kill -0 "$holder" 2>/dev/null; then
    return 1
  fi
  # Ohne PID (ältere Fassung, Craft-Zentrale) gilt sie nach 10 Minuten als verwaist.
  if [[ -z "$holder" && -z "$(find "$LOCK" -maxdepth 0 -mmin +10 2>/dev/null)" ]]; then
    return 1
  fi
  log "Verwaiste Sperre entfernt"
  rm -rf "$LOCK"
  mkdir "$LOCK" 2>/dev/null && echo $$ >"$LOCK/pid"
}
if ! take_lock; then
  log "Ein anderer Durchlauf ist gerade aktiv, dieser wird übersprungen"
  exit 0
fi
TMP="$(mktemp -d "${TMPDIR:-/tmp}/craft-update.XXXXXX")"
MOUNT=""
cleanup() {
  [[ -n "$MOUNT" ]] && /usr/bin/hdiutil detach -quiet "$MOUNT" >/dev/null 2>&1
  rm -rf "$TMP"
  rm -rf "$LOCK"
}
trap cleanup EXIT
trap 'exit 1' TERM INT HUP

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

# Sich selbst aktuell halten: die neueste Fassung dieses Skripts (mit der App-Liste) liegt im
# Repository der Craft-Zentrale. Sie gilt ab dem nächsten Durchlauf; eine fehlerhafte wird verworfen.
SELF_URL="https://raw.githubusercontent.com/$OWNER/craft-center/main/updater/craft-update.sh"
SELF_PATH="$STATE/craft-update.sh"
if /usr/bin/curl -fsSL --max-time 30 -o "$TMP/self.sh" "$SELF_URL" \
    && /bin/bash -n "$TMP/self.sh" && grep -q '^APPS=(' "$TMP/self.sh" \
    && ! cmp -s "$TMP/self.sh" "$SELF_PATH"; then
  cp "$TMP/self.sh" "$SELF_PATH.new" && chmod 755 "$SELF_PATH.new" && mv "$SELF_PATH.new" "$SELF_PATH" \
    && log "Updater: neue Fassung übernommen (gilt ab dem nächsten Durchlauf)"
fi

# Setzt die Standard-Apps aus DEFAULTS, wo sie abweichen; gibt je Änderung eine Zeile aus.
enforce_defaults() {
  local pairs=() entry uti id name
  for entry in "${DEFAULTS[@]}"; do
    IFS='=' read -r uti id name <<<"$entry"
    if [[ -d "/Applications/$name.app" || -d "$HOME/Applications/$name.app" ]]; then
      pairs+=("$uti=$id")
    fi
  done
  [[ ${#pairs[@]} -eq 0 ]] && return
  /usr/bin/osascript -l JavaScript - "${pairs[@]}" <<'JS'
function run(argv) {
  ObjC.import('CoreServices');
  const ALL = 0xFFFFFFFF; // kLSRolesAll
  const out = [];
  for (const pair of argv) {
    const [uti, app] = pair.split('=');
    let current = '';
    try {
      const ref = $.LSCopyDefaultRoleHandlerForContentType($(uti), ALL);
      current = ref ? ObjC.unwrap(ObjC.castRefToObject(ref)) || '' : '';
    } catch (e) { current = '?'; }
    if (current.toLowerCase() === app.toLowerCase()) continue;
    const err = $.LSSetDefaultRoleHandlerForContentType($(uti), ALL, $(app));
    out.push('Standard-App ' + uti + ': ' + (current || 'keine') + ' -> ' + app + (err === 0 ? '' : ' (Fehler ' + err + ')'));
  }
  return out.join('\n');
}
JS
}

log "Prüfe ${#APPS[@]} Apps"
for entry in "${APPS[@]}"; do
  update_app "${entry%%:*}" "${entry#*:}"
done

# Nach den Installationen, damit eine frisch installierte App gleich zugeordnet wird.
while IFS= read -r line; do
  [[ -n "$line" ]] && log "$line"
done < <(enforce_defaults 2>&1)
