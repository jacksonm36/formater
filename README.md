# Formater

Word (`.docx`) és Excel (`.xlsx`) szövegfeldolgozás Rustban: ismétlődő mondatok igazítása, beépített rövid angol + **teljes LibreOffice magyar** (`hu_HU.dic`, ~97k szótő) + IT kiegészítők (SymSpell), opcionális natív Hunspell, tanuló javítások (`learned_habits.json`). Részletek: [assets/HU_DICTIONARY_SOURCE.txt](assets/HU_DICTIONARY_SOURCE.txt).

## Előre fordított Windows binárisok

A [dist/windows-x86_64/](dist/windows-x86_64/) mappában:

| Fájl | Szerep |
|------|--------|
| `formater.exe` | Parancssor |
| `formater-gui.exe` | Grafikus felület |

Mellé tedd a `dicts` mappát (vagy hagyd üresen — van beépített szótár). Részletek: [dicts/README.txt](dicts/README.txt).

## Fordítás forrásból

```bash
cargo build --release
```

Kimenet: `target/release/formater.exe`, `target/release/formater-gui.exe`.

## Parancssor (példa)

```bash
formater -i dokumentum.docx
```

Opciók: `--no-spell`, `--no-learn`, `--learn-file`, `--git-diff`, stb. — lásd `formater --help`.

## Biztonság (röviden)

- **OOXML zip**: gyanús bejegyzésnevek (`..`, abszolút útvonal, `:`) és túl nagy (>80 MB) belső fájlok elutasítva (zip slip / zip bomb ellen).
- **Tanulási JSON**: max. ~4 MB fájl, korlátos bejegyzésszám és szóhossz; futás közben sem nő a térkép korlát nélkül.
- **Szerkesztési távolság**: a futó motor 1…5 közé szorítja (extrém CPU terhelés ellen).

Csak megbízható `.docx` / `.xlsx` fájlokat dolgozz fel; a program nem hálózatos.

## GitHub-ra feltöltés (első alkalommal)

1. Hozz létre egy **üres** repót a profilodon: [github.com/jacksonm36](https://github.com/jacksonm36?tab=repositories) → **New** → név pl. `formater` (ne README-vel, ne .gitignore-jal).
2. A projekt mappában (ha még nincs `origin`):

```bash
git remote add origin https://github.com/jacksonm36/formater.git
git branch -M main
git push -u origin main
```

(Más repónév esetén cseréld a URL végén a `formater` részt.)

## Licenc

Add meg a saját licencedet, ha kell.
