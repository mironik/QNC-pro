# QNC-pro: plan (2026-11-20)

## Zašto novi repo

Audit QNC-AI + QNC-shared (QNC-AI `docs/03-*` i `04-*`, 2026-11-20) pokazao je da je pravilo
"jedna radnja = jedna kocka" napušteno na tri načina: aplikacije i orkestratori odlučuju,
velike kocke udomljuju tuđe stanje, isto pravilo napisano više puta; plus "baza je jedina
istina" probijena JSON datotekama stanja i sirovim pisanjem mimo posrednika. Popravak unutar
postojećeg koda iznova lomi susjedne kocke. QNC-pro počinje prazan, s conformanceom od
prvog dana, i prima kocke jednu po jednu.

## Dvije grane (iz koda traya QNC-AI, `qnc-tray-mode`, `qnc-ingest-orchestrator`)

```
                 ┌─ Breaking news ─ AI Media Assist (odmah) ─┐
tray ─ kartica ─ uvoz ─┤                                        ├─ MA Script ─ S-Story ─ Story ─ Export
                 └─ Story ─ (čeka prijepis/val/filmstrip) ─ Ingest ─ AI Media Assist ─┘
                 └─ Net ─ uvoz + AI, bez forme
```

Način rada računala i korak priče su zapisi u bazi (registar računala, baza projekta), ne
`tray-mode.json` ni memorija traya. Tray ih čita kroz jednu kocku.

## Slojevi

```
apps/        samo main: pokreni formu
crates/*     kind u ugovoru: block | module | panel | form | orchestrator
contracts/   modules/*.module.json, keyboard-shortcuts.json, text/*.json, ui/*.layout.json
tools/       conformance i pomoćni programi (nisu kocke)
docs/        plan, audit, razvoj/<dan>.md
```

## Koraci

| # | Korak | Ulaz iz QNC-shared/QNC-AI | Izlaz |
|---|---|---|---|
| 0 | Kostur: AGENTS, conformance, workspace | - | ovaj commit |
| 1 | Temelj: `qnc-diagnostics` (jedan log), `qnc-text`, `qnc-theme`, `qnc-ui-kit`, `qnc-actions` (katalog), `qnc-settings` (postavke računala, jedina kocka s `paths`), `qnc-launch` (jedino `Command::new`), `qnc-db` (posrednik, jedini pisac) | qnc-dev-diagnostics, qnc-ui-text, qnc-desktop-theme, qnc-ui-kit, qnc-keyboard-shortcut, qnc-plugin-process, qnc-db-broker | prolaze prijemni popis |
| 2 | `qnc-marks` (uzeti IN/OUT, jedan vlasnik: uzmi, pomakni, I/O, spremi, odustani, zatvori) + zlatna tablica tipki i miša | qnc-source-mark-focus, dio qnc-source-panel | greška "Enter vraća oznaku" ne može nastati |
| 3 | `qnc-voice-gate` (jedan prag glasa), `qnc-wave-read` (jedan čitač vala po formi), `qnc-letters` (pravilo slova kod vlasnika) | qnc-word-align, qnc-fine-wave, qnc-letter-db/-times | |
| 4 | `qnc-source-timeline` modul + `qnc-zoom` modul (os od vlasnika; slike, zvuk, riječi, player kao kocke) | qnc-timeline, qnc-program-view zoom_axis, qnc-trim-overlay | zoom izvan priče |
| 5 | Player preview rastavljen: sesija, program, Sync, normalizacija, čekanje zapisa | qnc-source-preview | |
| 6 | Tray grana: `qnc-disks`, `qnc-card`, `qnc-progress`, `qnc-eject`, `qnc-mode` (u bazi), `qnc-story-step` (u bazi); tray i uvoz bez pravila | qnc-attached-cards, qnc-work-progress, qnc-ingest-base, qnc-tray | obje grane rade do AI MA |
| 7 | Project KOPIRAN iz QNC-AI i rastavljen: store (registar, templatei, kreiranje, brisanje, zatvaranje), jobs, application, session, blocks kao paneli, desktop kao forma; obrazac priloga, settings-form, lokacije, AI postavke, dir-browser; isti layout i look | qnc-project-store, qnc-project-blocks, qnc-project-session, qnc-project-jobs, qnc-newsroom-sheet*, qnc-settings-*, qnc-dir-browser, qnc-ai-features* | kits: board, confirm, tema (gotovo) |
| 8 | Script i priča: jedan undo, jedan automat slip/trim/level/assist; Send to S-Story u kocki priče | qnc-script, qnc-program-segments, qnc-subsegment-edit | |
| 9 | Forme: AI Media Assist, Ingest, MA Script, S-Story, Story, Export; svaka kad je prethodna čista | layout ugovori | |

Svaki korak: zlatni test ponašanja (gdje postoji u starom kodu), prijenos, isti test, commit.
Što ne prolazi prijemni popis, rastavlja se ovdje prije prijenosa.
