Hunspell szótárak (.dic + .aff)

A program a .dic szólistát használja (az .aff fájl jelenleg nincs feldolgozva).

Ajánlott fájlnevek ebben a mappában:
  hu_HU.dic  — magyar
  en_US.dic  — angol (US)
  en_GB.dic  — angol (UK), ha nincs en_US

Letöltés: LibreOffice / Mozilla Hunspell csomagokból másold ide a .dic (és később használható .aff) fájlokat.

Egyéni útvonal:  formater --dic-hu C:\path\hu_HU.dic --dic-en C:\path\en_US.dic

Natív Hunspell (.aff támogatás)
  Fordítás: cargo build --release --features hunspell
  Futtatás:   formater -i f.docx --native-hunspell
  Minden megadott .dic mellé kell ugyanolyan nevű .aff (pl. hu_HU.dic + hu_HU.aff).
  Megjegyzés: a hunspell-sys bindgen miatt Windows alatt LLVM / libclang kell
  (LIBCLANG_PATH), vagy Linuxon/WSL-en egyszerűbb a fordítás.

Word: --docx-merge-runs
  Alapértelmezés: több szövegfutás (félkövér/részek) megmarad; ilyen bekezdésben
  nincs mondat-szintű duplikáció-igazítás, csak futásonkénti javítás.
  --docx-merge-runs: régi viselkedés (egy futásba összevonás).

Excel: a munkafüzetben egy közös mondat-térkép (sharedStrings + lapok inline
  szövegei, sheet1, sheet2, … sorrendben).

GUI: cargo build --release → target\release\formater-gui.exe
  (A parancssoros eszköz továbbra is: formater.exe)

Beépített szótár: az exe a projekt assets/ mappájából beágyazott .dic fájlokat tölt
  (Hunspell-szerű első sor + szavak). Sorrend: alap angol (embedded_en.dic),
  teljes magyar LibreOffice hu_HU.dic (assets/hu_HU_libreoffice.dic — lásd
  assets/HU_DICTIONARY_SOURCE.txt), majd egy kis SymSpell-kiegészítő
  (embedded_hu_supplement.dic), aztán IT / hálózat / biztonság kiegészítők
  (embedded_tech_en.dic, embedded_tech_hu.dic — router, tűzfal, VLAN, stb.).
  A helyesírás így külső fájl nélkül is működik; a dicts/ mappa továbbra is
  bővítheti (opcionális .dic útvonalak ezek után egyesülnek; teljes Hunspell
  .dic ajánlott komoly használathoz).

Tanulás (személyre szabott javítások):
  Ha a helyesírás be van kapcsolva, a program ugyanarra a szóra ugyanazt a
  szótár-javítást többször látva (alapból 2×) eltárolja, és legközelebb a
  szótár előtt alkalmazza. Fájl: Windows alatt %APPDATA%\formater\learned_habits.json
  (más OS: XDG config vagy ~/.config/formater/). CLI: --learn-file útvonal,
  kikapcsolás: --no-learn. A JSON szerkeszthető; „corrections” = végleges párok.

Git-stílusú diff: a Word/Excel fájlból kinyert szövegsorokra unified diff
  (mint git diff -u). CLI: --git-diff. GUI: jelölőnégyzet.
