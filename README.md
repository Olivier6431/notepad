# Notepad

A lightweight, multi-tab text editor built with **Rust** and [iced](https://github.com/iced-rs/iced).

**[Lire en fran&ccedil;ais](README.fr.md)**

---

## Features

### Tabs
- Multi-tab editing with `Ctrl+N`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+Shift+Tab`
- Session restoration: reopen tabs and unsaved content on startup
- Drag & drop file opening

### Editing
- Undo / Redo (`Ctrl+Z` / `Ctrl+Y`) with smart batching and adaptive history
- Cut / Copy / Paste (`Ctrl+X` / `Ctrl+C` / `Ctrl+V`)
- Select All (`Ctrl+A`)
- Insert Date/Time (`F5`)
- Right-click context menu

### Search & Replace
- Find (`Ctrl+F`), Replace (`Ctrl+H`), Go to Line (`Ctrl+G`)
- Regex support with case sensitivity toggle
- Find Next (`F3`) / Find Previous (`Shift+F3`) with wrap-around

### View
- Dark / Light theme
- Word wrap toggle (`Alt+Z`)
- Zoom In/Out/Reset (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`, or `Ctrl+Mouse Wheel`)
- Line numbers, custom scrollbar

### Format
- Font family selection (Consolas, Courier New, Cascadia Code, Lucida Console, Segoe UI, Arial, Times New Roman)
- Adjustable font size (8 - 40pt)

### File Handling
- Auto-save every 30 seconds
- External file change detection with reload/ignore prompt
- Encoding auto-detection: UTF-8, UTF-16 (BOM), Windows-1252 fallback
- Line ending detection (LF / CRLF)
- Large file support (warning at 50 MB, limit at 500 MB)

### Status Bar
- Cursor position (line, column)
- Selected characters count
- Word count, character count, line count
- Zoom level, line ending, encoding

### Preferences
- All settings persisted in `preferences.json` (theme, font, word wrap, window size, session restore)

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+N` | New tab |
| `Ctrl+O` | Open |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save As |
| `Ctrl+W` | Close tab |
| `Ctrl+Z` | Undo |
| `Ctrl+Y` | Redo |
| `Ctrl+X` | Cut |
| `Ctrl+C` | Copy |
| `Ctrl+V` | Paste |
| `Ctrl+A` | Select All |
| `Ctrl+F` | Find |
| `Ctrl+H` | Replace |
| `Ctrl+G` | Go to Line |
| `F3` | Find Next |
| `Shift+F3` | Find Previous |
| `F5` | Insert Date/Time |
| `Alt+Z` | Toggle Word Wrap |
| `Ctrl+=` | Zoom In |
| `Ctrl+-` | Zoom Out |
| `Ctrl+0` | Zoom Reset |
| `Ctrl+Tab` | Next tab |
| `Ctrl+Shift+Tab` | Previous tab |
| `Escape` | Close panel |

---

## Build

```bash
cargo build --release
```

The binary will be in `target/release/notepad.exe`.

---

## License

[GPL-3.0](LICENSE)
