# Notepad

Un éditeur de texte léger et multi-onglets construit avec **Rust** et [iced](https://github.com/iced-rs/iced).

**[Read in English](README.md)**

---

## Fonctionnalités

### Onglets
- Édition multi-onglets avec `Ctrl+N`, `Ctrl+W`, `Ctrl+Tab`, `Ctrl+Shift+Tab`
- Restauration de session : réouverture des onglets et du contenu non enregistré au démarrage
- Ouverture de fichiers par glisser-déposer

### Édition
- Annuler / Rétablir (`Ctrl+Z` / `Ctrl+Y`) avec regroupement intelligent et historique adaptatif
- Couper / Copier / Coller (`Ctrl+X` / `Ctrl+C` / `Ctrl+V`)
- Tout sélectionner (`Ctrl+A`)
- Insérer date/heure (`F5`)
- Menu contextuel (clic droit)

### Recherche et remplacement
- Rechercher (`Ctrl+F`), Remplacer (`Ctrl+H`), Aller à la ligne (`Ctrl+G`)
- Support des expressions régulières avec bascule de sensibilité à la casse
- Suivant (`F3`) / Précédent (`Shift+F3`) avec bouclage

### Affichage
- Thème sombre / clair
- Retour à la ligne (`Alt+Z`)
- Zoom avant/arrière/réinitialiser (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`, ou `Ctrl+Molette`)
- Numéros de ligne, barre de défilement personnalisée

### Format
- Choix de la police (Consolas, Courier New, Cascadia Code, Lucida Console, Segoe UI, Arial, Times New Roman)
- Taille de police ajustable (8 - 40pt)

### Gestion des fichiers
- Sauvegarde automatique toutes les 30 secondes
- Détection des modifications externes avec option de rechargement
- Détection automatique de l'encodage : UTF-8, UTF-16 (BOM), Windows-1252
- Détection des fins de ligne (LF / CRLF)
- Support des fichiers volumineux (avertissement à 50 Mo, limite à 500 Mo)

### Barre de statut
- Position du curseur (ligne, colonne)
- Nombre de caractères sélectionnés
- Nombre de mots, de caractères, de lignes
- Niveau de zoom, fin de ligne, encodage

### Préférences
- Tous les paramètres sauvegardés dans `preferences.json` (thème, police, retour à la ligne, taille de fenêtre, restauration de session)

---

## Raccourcis clavier

| Raccourci | Action |
|---|---|
| `Ctrl+N` | Nouvel onglet |
| `Ctrl+O` | Ouvrir |
| `Ctrl+S` | Enregistrer |
| `Ctrl+Shift+S` | Enregistrer sous |
| `Ctrl+W` | Fermer l'onglet |
| `Ctrl+Z` | Annuler |
| `Ctrl+Y` | Rétablir |
| `Ctrl+X` | Couper |
| `Ctrl+C` | Copier |
| `Ctrl+V` | Coller |
| `Ctrl+A` | Tout sélectionner |
| `Ctrl+F` | Rechercher |
| `Ctrl+H` | Remplacer |
| `Ctrl+G` | Aller à la ligne |
| `F3` | Occurrence suivante |
| `Shift+F3` | Occurrence précédente |
| `F5` | Insérer date/heure |
| `Alt+Z` | Retour à la ligne |
| `Ctrl+=` | Zoom avant |
| `Ctrl+-` | Zoom arrière |
| `Ctrl+0` | Réinitialiser le zoom |
| `Ctrl+Tab` | Onglet suivant |
| `Ctrl+Shift+Tab` | Onglet précédent |
| `Escape` | Fermer le panneau |

---

## Compilation

```bash
cargo build --release
```

Le binaire sera dans `target/release/notepad.exe`.

---

## Licence

[GPL-3.0](LICENSE)
