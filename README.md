# Notepad

A lightweight, multi-tab text editor built with **Rust** and [iced](https://github.com/iced-rs/iced).

Un editeur de texte leger et multi-onglets construit avec **Rust** et [iced](https://github.com/iced-rs/iced).

---

## Features / Fonctionnalites

### Tabs / Onglets
- Multi-tab editing with `Ctrl+N`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+Shift+Tab`
- Session restoration: reopen tabs and unsaved content on startup
- Drag & drop file opening

### Editing / Edition
- Undo / Redo (`Ctrl+Z` / `Ctrl+Y`) with smart batching and adaptive history
- Cut / Copy / Paste (`Ctrl+X` / `Ctrl+C` / `Ctrl+V`)
- Select All (`Ctrl+A`)
- Insert Date/Time (`F5`)
- Right-click context menu

### Search & Replace / Recherche & Remplacement
- Find (`Ctrl+F`), Replace (`Ctrl+H`), Go to Line (`Ctrl+G`)
- Regex support with case sensitivity toggle
- Find Next (`F3`) / Find Previous (`Shift+F3`) with wrap-around

### View / Affichage
- Dark / Light theme
- Word wrap toggle (`Alt+Z`)
- Zoom In/Out/Reset (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`, or `Ctrl+Mouse Wheel`)
- Line numbers, custom scrollbar

### Format
- Font family selection (Consolas, Courier New, Cascadia Code, Lucida Console, Segoe UI, Arial, Times New Roman)
- Adjustable font size (8 - 40pt)

### File Handling / Gestion des fichiers
- Auto-save every 30 seconds
- External file change detection with reload/ignore prompt
- Encoding auto-detection: UTF-8, UTF-16 (BOM), Windows-1252 fallback
- Line ending detection (LF / CRLF)
- Large file support (warning at 50 MB, limit at 500 MB)

### Status Bar / Barre de statut
- Cursor position (line, column)
- Selected characters count
- Word count, character count, line count
- Zoom level, line ending, encoding

### Preferences
- All settings persisted in `preferences.json` (theme, font, word wrap, window size, session restore)

---

## Keyboard Shortcuts / Raccourcis clavier

| Shortcut | Action |
|---|---|
| `Ctrl+N` | New tab / Nouvel onglet |
| `Ctrl+O` | Open / Ouvrir |
| `Ctrl+S` | Save / Enregistrer |
| `Ctrl+Shift+S` | Save As / Enregistrer sous |
| `Ctrl+W` | Close tab / Fermer l'onglet |
| `Ctrl+Z` | Undo / Annuler |
| `Ctrl+Y` | Redo / Retablir |
| `Ctrl+X` | Cut / Couper |
| `Ctrl+C` | Copy / Copier |
| `Ctrl+V` | Paste / Coller |
| `Ctrl+A` | Select All / Tout selectionner |
| `Ctrl+F` | Find / Rechercher |
| `Ctrl+H` | Replace / Remplacer |
| `Ctrl+G` | Go to Line / Aller a la ligne |
| `F3` | Find Next / Suivant |
| `Shift+F3` | Find Previous / Precedent |
| `F5` | Insert Date/Time / Inserer date/heure |
| `Alt+Z` | Toggle Word Wrap / Retour a la ligne |
| `Ctrl+=` | Zoom In |
| `Ctrl+-` | Zoom Out |
| `Ctrl+0` | Zoom Reset |
| `Ctrl+Tab` | Next tab / Onglet suivant |
| `Ctrl+Shift+Tab` | Previous tab / Onglet precedent |
| `Escape` | Close panel / Fermer le panneau |

---

## Build / Compilation

```bash
cargo build --release
```

The binary will be in `target/release/notepad.exe`.

---

## License

[GPL-3.0](LICENSE)
