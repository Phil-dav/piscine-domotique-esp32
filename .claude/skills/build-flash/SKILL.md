---
name: build-flash
description: Compiler, vérifier et flasher le firmware Rust ESP32 piscine (c:\rust\3). À utiliser dès que l'utilisateur demande de compiler, vérifier, flasher ou tester le firmware sur ce dépôt.
---

# Build & flash — firmware Rust ESP32 piscine

Cible : `xtensa-esp32-espidf` (voir `.cargo/config.toml`). Toolchain ESP-IDF `v5.5.3`.

## Commandes

- Vérification rapide (sans lien final) : `cargo check`
- Build complet (matche la CI, `.github/workflows/*.yml`) : `cargo build --release`
- Formatage : `cargo fmt --all -- --check`
- Lints stricts (la CI échoue sur tout warning) : `cargo clippy --all-targets --all-features --workspace -- -D warnings`
- Flash + moniteur série (nécessite l'ESP32 branché) : `cargo run` (utilise le runner `espflash flash --monitor` défini dans `.cargo/config.toml`)

Ces quatre premières commandes sont censées passer sans erreur avant de considérer une tâche terminée — ce sont exactement les jobs de la CI GitHub Actions (`rust-checks`).

## Notes

- Le premier `cargo check`/`build` peut être long (compilation de `esp-idf-sys` / toolchain ESP-IDF) — prévoir plusieurs minutes, lancer en arrière-plan si possible.
- Ne jamais flasher (`cargo run`, `espflash flash`) sans confirmation explicite de l'utilisateur si un ESP32 est réellement branché — c'est une action physique sur du matériel réel.
- Avant de flasher du code qui touche à la logique du relais pompe, le mot-clé **« sécurité-pompe »** doit avoir été prononcé explicitement par l'utilisateur dans la conversation (règle héritée du projet C++ d'origine).
