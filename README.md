# Craft-Zentrale

Kleine Mac-App für die angepassten ArtCraft-Apps (Forks unter github.com/76344243):

1. **Fork auf GitHub:** neueste Fork-Version und ob das Original seitdem neue Änderungen hat.
2. **Installiert:** welche Fork-Version auf diesem Mac liegt.
3. **Übereinstimmung** und ein **Update**-Knopf mit Live-Anzeige (Herunterladen, Installieren, erfolgreich).

`updater/` enthält den ArtCraft-Updater (LaunchAgent), der neue Versionen automatisch installiert;
beide teilen sich Vermerke und Sperre in `~/Library/Application Support/ArtCraft-Updater`.
Eine weitere App kommt in `APPS` in `src/core.rs` **und** in `updater/craft-update.sh`.

Jeder Push auf `main` baut per GitHub Actions eine DMG und veröffentlicht sie als Release.
`CRAFT_CENTER_DEMO=1 cargo run` zeigt das Fenster mit Beispieldaten.
